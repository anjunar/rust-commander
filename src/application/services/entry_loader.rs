use anyhow::Result;

use crate::{
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
    show_hidden_files: bool,
}

impl EntryLoader {
    pub fn new(
        archive_service: ArchiveService,
        remote_service: RemoteService,
        show_hidden_files: bool,
    ) -> Self {
        Self {
            archive_service,
            remote_service,
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
            PanelLocation::Archive(_) => Ok(self.archive_service.entries_for_location(location)?),
            PanelLocation::Remote(location) => self
                .remote_service
                .read_entries(location, self.show_hidden_files),
        }
    }
}
