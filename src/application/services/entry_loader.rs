use anyhow::Result;

use crate::{
    application::SessionStore,
    archive::ArchiveService,
    domain::{Entry, PanelLocation},
    remote::RemoteService,
};

#[derive(Clone, Debug)]
pub struct EntryLoadResult {
    pub location: PanelLocation,
    pub entries: Vec<Entry>,
}

#[derive(Clone, Debug)]
pub struct EntryLoader {
    archive_service: ArchiveService,
    remote_service: RemoteService,
    session_store: SessionStore,
    show_hidden_files: bool,
}

impl EntryLoader {
    pub fn new(
        archive_service: ArchiveService,
        remote_service: RemoteService,
        session_store: SessionStore,
        show_hidden_files: bool,
    ) -> Self {
        Self {
            archive_service,
            remote_service,
            session_store,
            show_hidden_files,
        }
    }

    pub fn load(&self, requested_location: PanelLocation) -> Result<EntryLoadResult> {
        let entries = self.load_entries(&requested_location)?;
        Ok(EntryLoadResult {
            location: requested_location,
            entries,
        })
    }

    fn load_entries(&self, location: &PanelLocation) -> Result<Vec<Entry>> {
        match location {
            PanelLocation::Filesystem(path) => {
                crate::fs::reader::read_entries(path, self.show_hidden_files)
            }
            PanelLocation::Archive(view) => {
                let session = self
                    .session_store
                    .archive(&view.session_key)
                    .ok_or_else(|| anyhow::anyhow!("Archive session not found"))?;
                Ok(self.archive_service.entries_for_archive_view(view, &session))
            }
            PanelLocation::Remote(location) => {
                let session = self
                    .session_store
                    .remote(&location.session_key)
                    .ok_or_else(|| anyhow::anyhow!("Remote session not found"))?;
                self.remote_service.read_entries(
                    &session,
                    &location.current_path,
                    self.show_hidden_files,
                )
            }
        }
    }
}
