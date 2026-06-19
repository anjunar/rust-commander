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
        let location = self.resolve_location(requested_location)?;
        let entries = self.load_entries(&location)?;
        Ok(EntryLoadResult { location, entries })
    }

    fn resolve_location(
        &self,
        requested_location: PanelLocation,
    ) -> Result<PanelLocation, ArchiveError> {
        match &requested_location {
            PanelLocation::Filesystem(path)
                if self.archive_service.is_archive_path(path) && path.is_file() =>
            {
                self.archive_service.archive_location_for_path(path)
            }
            _ => Ok(requested_location),
        }
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
