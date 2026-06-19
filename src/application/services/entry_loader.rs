use crate::{
    archive::{ArchiveError, ArchiveService},
    domain::{Entry, PanelLocation},
};

#[derive(Clone, Debug)]
pub struct EntryLoadResult {
    pub location: PanelLocation,
    pub entries: Vec<Entry>,
}

#[derive(Clone, Debug)]
pub struct EntryLoader {
    archive_service: ArchiveService,
    show_hidden_files: bool,
}

impl EntryLoader {
    pub fn new(archive_service: ArchiveService, show_hidden_files: bool) -> Self {
        Self {
            archive_service,
            show_hidden_files,
        }
    }

    pub fn load(&self, requested_location: PanelLocation) -> Result<EntryLoadResult, ArchiveError> {
        let entries = self.load_entries(&requested_location)?;
        Ok(EntryLoadResult {
            location: requested_location,
            entries,
        })
    }

    fn load_entries(&self, location: &PanelLocation) -> Result<Vec<Entry>, ArchiveError> {
        match location {
            PanelLocation::Filesystem(path) => {
                crate::fs::reader::read_entries(path, self.show_hidden_files).map_err(|error| {
                    ArchiveError::IoError {
                        detail: error.to_string(),
                    }
                })
            }
            PanelLocation::Archive(_) => self.archive_service.entries_for_location(location),
        }
    }
}
