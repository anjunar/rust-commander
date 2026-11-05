use std::{
    fs,
    net::{TcpStream, ToSocketAddrs},
    path::Path,
};

use anyhow::{anyhow, bail, Context, Result};
use ssh2::{CheckResult, KnownHostFileKind, Session};

use crate::remote::{RemoteAuthConfig, RemoteProfile, RemoteRuntimeSecret, RemoteSession};

use super::{
    util::{
        describe_auth_error, describe_connect_error, describe_ssh_session_error,
        find_known_hosts_path, format_fingerprint, known_hosts_entry_name, known_hosts_write_path,
    },
    ConnectedRemote, HostKeyAlgorithm, HostKeyTrustOffer, HostKeyUntrusted, RemoteService,
    CONNECT_TIMEOUT, SSH_SESSION_TIMEOUT, SSH_SESSION_TIMEOUT_MS,
};

impl RemoteService {
    pub(super) fn connect_sftp(&self, remote_session: &RemoteSession) -> Result<ConnectedRemote> {
        let profile = remote_session.profile();
        let tcp = connect_tcp(profile)?;
        let mut session = Session::new().context("Could not create SSH session")?;
        session.set_timeout(SSH_SESSION_TIMEOUT_MS);
        session.set_tcp_stream(tcp);
        session.handshake().map_err(|error| {
            anyhow!(describe_ssh_session_error(
                "complete the SSH handshake",
                &error
            ))
        })?;
        if !profile.skip_host_key_verification {
            verify_known_host(&session, profile)?;
        }

        match (&profile.auth, remote_session.secret()) {
            (RemoteAuthConfig::Password { username }, RemoteRuntimeSecret::Password(password)) => {
                session
                    .userauth_password(username, password)
                    .map_err(|error| {
                        anyhow!(describe_auth_error("password", username, profile, &error))
                    })?;
            }
            (
                RemoteAuthConfig::KeyFile {
                    username,
                    private_key_path,
                    public_key_path,
                },
                RemoteRuntimeSecret::KeyPassphrase(passphrase),
            ) => {
                authenticate_with_key_file(
                    &session,
                    username,
                    private_key_path,
                    public_key_path.as_deref(),
                    Some(passphrase),
                    profile,
                )?;
            }
            (
                RemoteAuthConfig::KeyFile {
                    username,
                    private_key_path,
                    public_key_path,
                },
                RemoteRuntimeSecret::None,
            ) => {
                authenticate_with_key_file(
                    &session,
                    username,
                    private_key_path,
                    public_key_path.as_deref(),
                    None,
                    profile,
                )?;
            }
            (RemoteAuthConfig::Password { .. }, _) => {
                bail!("A password is required for this remote profile");
            }
            (RemoteAuthConfig::KeyFile { .. }, _) => {
                bail!("This key-based profile requires a passphrase or no passphrase");
            }
        }

        if !session.authenticated() {
            bail!(
                "Authentication failed for {}@{}:{}",
                profile.auth.username(),
                profile.host,
                profile.port
            );
        }

        let sftp = session.sftp().context("Could not start SFTP session")?;
        Ok(ConnectedRemote {
            _session: session,
            sftp,
        })
    }
}

fn connect_tcp(profile: &RemoteProfile) -> Result<TcpStream> {
    let address = format!("{}:{}", profile.host, profile.port);
    let resolved = (profile.host.as_str(), profile.port)
        .to_socket_addrs()
        .with_context(|| format!("Could not resolve remote host {address}"))?
        .collect::<Vec<_>>();
    if resolved.is_empty() {
        bail!("Could not resolve remote host {address}");
    }

    let mut last_error = None;
    for socket_address in resolved {
        match TcpStream::connect_timeout(&socket_address, CONNECT_TIMEOUT) {
            Ok(stream) => {
                stream
                    .set_read_timeout(Some(SSH_SESSION_TIMEOUT))
                    .with_context(|| format!("Could not configure read timeout for {address}"))?;
                stream
                    .set_write_timeout(Some(SSH_SESSION_TIMEOUT))
                    .with_context(|| format!("Could not configure write timeout for {address}"))?;
                return Ok(stream);
            }
            Err(error) => last_error = Some(error),
        }
    }

    let error = last_error
        .unwrap_or_else(|| std::io::Error::other(format!("No reachable address for {address}")));
    bail!("{}", describe_connect_error(&address, &error));
}

fn verify_known_host(session: &Session, profile: &RemoteProfile) -> Result<()> {
    let offer = host_key_trust_offer(session, profile)?;

    let Some(known_hosts_path) = find_known_hosts_path() else {
        return Err(anyhow!(HostKeyUntrusted(offer)));
    };

    let mut known_hosts = session
        .known_hosts()
        .context("Could not initialize SSH known_hosts verification")?;
    known_hosts
        .read_file(&known_hosts_path, KnownHostFileKind::OpenSSH)
        .with_context(|| {
            format!(
                "Could not read SSH known_hosts file {}",
                known_hosts_path.display()
            )
        })?;

    match known_hosts.check_port(&profile.host, profile.port, &offer.host_key) {
        CheckResult::Match => Ok(()),
        CheckResult::NotFound => Err(anyhow!(HostKeyUntrusted(offer))),
        CheckResult::Mismatch => bail!(
            "SSH host key mismatch for {}:{} against {}. \
This may indicate a server change or a man-in-the-middle risk. Server fingerprint: SHA256:{}",
            profile.host,
            profile.port,
            known_hosts_path.display(),
            offer.fingerprint_sha256
        ),
        CheckResult::Failure => bail!(
            "SSH host key verification failed for {}:{} using {}",
            profile.host,
            profile.port,
            known_hosts_path.display()
        ),
    }
}

fn host_key_trust_offer(session: &Session, profile: &RemoteProfile) -> Result<HostKeyTrustOffer> {
    let (host_key, key_type) = session
        .host_key()
        .ok_or_else(|| anyhow!("SSH server did not provide a host key during handshake"))?;
    let fingerprint = session
        .host_key_hash(ssh2::HashType::Sha256)
        .map(format_fingerprint)
        .unwrap_or_else(|| "<unavailable>".into());
    Ok(HostKeyTrustOffer {
        host: profile.host.clone(),
        port: profile.port,
        fingerprint_sha256: fingerprint,
        host_key: host_key.to_vec(),
        key_type: HostKeyAlgorithm::from_ssh2(key_type),
    })
}

pub(super) fn trust_host_key(offer: &HostKeyTrustOffer) -> Result<()> {
    if matches!(offer.key_type, HostKeyAlgorithm::Unknown) {
        bail!(
            "Cannot trust SSH host key for {}:{}: unsupported key type",
            offer.host,
            offer.port
        );
    }

    let known_hosts_path = known_hosts_write_path()
        .context("Could not resolve OpenSSH known_hosts path under the user home directory")?;
    if let Some(parent) = known_hosts_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Could not create SSH directory {}", parent.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
        }
    }

    let session = Session::new().context("Could not create SSH session for known_hosts update")?;
    let mut known_hosts = session
        .known_hosts()
        .context("Could not initialize SSH known_hosts for writing")?;
    if known_hosts_path.is_file() {
        known_hosts
            .read_file(&known_hosts_path, KnownHostFileKind::OpenSSH)
            .with_context(|| {
                format!(
                    "Could not read SSH known_hosts file {}",
                    known_hosts_path.display()
                )
            })?;
    }

    let host_name = known_hosts_entry_name(&offer.host, offer.port);
    known_hosts
        .add(
            &host_name,
            &offer.host_key,
            "rust-commander",
            offer.key_type.to_known_host_format(),
        )
        .with_context(|| format!("Could not add {}:{} to known_hosts", offer.host, offer.port))?;
    known_hosts
        .write_file(&known_hosts_path, KnownHostFileKind::OpenSSH)
        .with_context(|| {
            format!(
                "Could not write SSH known_hosts file {}",
                known_hosts_path.display()
            )
        })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&known_hosts_path, fs::Permissions::from_mode(0o600));
    }

    Ok(())
}

fn ensure_key_files_exist(private_key_path: &Path, public_key_path: Option<&Path>) -> Result<()> {
    if !private_key_path.is_file() {
        bail!(
            "Private key file was not found: {}",
            private_key_path.display()
        );
    }
    if let Some(public_key_path) = public_key_path {
        if !public_key_path.is_file() {
            bail!(
                "Public key file was not found: {}",
                public_key_path.display()
            );
        }
    }
    Ok(())
}

fn authenticate_with_key_file(
    session: &Session,
    username: &str,
    private_key_path: &Path,
    public_key_path: Option<&Path>,
    passphrase: Option<&str>,
    profile: &RemoteProfile,
) -> Result<()> {
    ensure_key_files_exist(private_key_path, public_key_path)?;

    let file_error =
        match session.userauth_pubkey_file(username, public_key_path, private_key_path, passphrase)
        {
            Ok(()) => return Ok(()),
            Err(error) => error,
        };

    // On Windows, libssh2 defaults to WinCNG which cannot load OpenSSH Ed25519 keys
    // (auth fails with "unknown error"). Fall back to the system ssh-agent, which can
    // sign with those keys after `ssh-add`.
    match authenticate_with_agent(session, username) {
        Ok(()) => Ok(()),
        Err(agent_error) => {
            let mut message = describe_auth_error("public key", username, profile, &file_error);
            message.push_str(&format!(
                "\nSSH agent fallback also failed: {agent_error}\n\
On Windows, Ed25519 OpenSSH keys often cannot be loaded from a file. \
Start the OpenSSH Authentication Agent service, run `ssh-add` for your key, \
or use an RSA key instead."
            ));
            bail!(message)
        }
    }
}

fn authenticate_with_agent(session: &Session, username: &str) -> Result<()> {
    let mut agent = session
        .agent()
        .context("Could not create SSH agent handle")?;
    agent.connect().context(
        "Could not connect to SSH agent (is the OpenSSH Authentication Agent service running?)",
    )?;
    agent
        .list_identities()
        .context("Could not list identities from SSH agent")?;
    let identities = agent
        .identities()
        .context("Could not read identities from SSH agent")?;
    if identities.is_empty() {
        bail!("no identities found in the SSH agent (try: ssh-add path\\to\\private_key)");
    }

    let mut last_error = None;
    for identity in &identities {
        match agent.userauth(username, identity) {
            Ok(()) => return Ok(()),
            Err(error) => last_error = Some(error),
        }
    }

    match last_error {
        Some(error) => bail!("{}", error.message()),
        None => bail!("SSH agent authentication failed"),
    }
}
