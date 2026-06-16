# Windows Terminal Strategy

Status as of 2026-06-16: selected direction for Windows is a native sidecar host, not a GTK-rendered ConPTY terminal.

## Why ConPTY alone is not enough

Microsoft's pseudoconsole documentation is explicit: ConPTY gives the host application a terminal data stream, but the host must still render terminal output and collect terminal input itself.

That means a GTK4 + Rust app would still need to provide:

- VT parsing and rendering
- cursor movement and resize behavior
- selection and clipboard behavior
- keyboard encoding and control-sequence input
- scrollback, reflow, and screen-buffer behavior

That is exactly the class of custom terminal emulation RCommander should avoid.

## Evaluated options

### 1. Build directly on ConPTY inside GTK

Rejected.

Reason:

- technically possible in principle,
- but it would force RCommander to ship its own terminal renderer or to adopt a non-native rendering stack inside the GTK widget tree,
- which conflicts with the project's explicit "no homemade terminal emulation" rule.

### 2. Wait for a reusable Windows Terminal control and host it natively

Selected direction.

Reason:

- Microsoft documents ConPTY as a stream-hosting API, not a complete embeddable terminal widget.
- The Windows Terminal repository has long-term plans for reusable terminal controls, but those controls are not documented as a stable, productized app-embedding surface yet.
- Microsoft also documents XAML Islands and low-level XAML hosting as the Windows-supported way to host XAML controls in desktop apps, which aligns with a native Windows sidecar approach far better than trying to force WinUI/XAML into the GTK widget tree.

## Chosen architecture

### Short term

Keep the current GTK bottom dock and focus behavior exactly as the cross-platform shell of the feature.

On Windows, the dock remains the coordination surface for:

- toggle
- focus
- restart in active panel directory
- close
- current panel path

The embedded area remains a placeholder until a real native control is selected.

### Medium term

Create a separate Windows-only native host application, for example:

- `rcommander-terminal-host-wpf`
- or `rcommander-terminal-host-winui`

Responsibilities of that host:

- own the Windows-native terminal control or future productized TerminalControl
- manage shell lifecycle
- start the shell in the requested directory
- expose a narrow IPC surface back to RCommander

Suggested IPC commands:

- `show`
- `hide`
- `focus`
- `restart { cwd }`
- `clear`
- `set-title`
- `get-cwd`

RCommander would continue to own commander state and shortcut routing, while the sidecar owns terminal rendering.

### Long term

If Microsoft ships a supported reusable Windows Terminal control with a stable embedding story, replace the placeholder backend with an adapter that talks to that control. The existing `TerminalDock` and `TerminalController` split is already aligned for that swap.

## What we should not do

- Do not write a custom VT renderer in GTK.
- Do not fake an interactive terminal with a text box plus stdin/stdout pipes.
- Do not try to re-parent `conhost.exe` or `cmd.exe` windows into GTK.
- Do not promise an embedded Windows terminal before a supported native control is chosen.

## Immediate next implementation step

The next real Windows milestone is not more GTK work. It is a small proof-of-concept native host outside the Rust GTK process that answers one question:

"Can we host a supported Windows-native terminal control in a companion process and drive it through a minimal IPC contract?"

If yes, we integrate that companion host.
If no, we keep the current placeholder and expose Windows Terminal as an external tool instead of pretending there is an embedded backend.
