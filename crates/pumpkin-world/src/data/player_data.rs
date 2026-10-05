use pumpkin_nbt::compound::NbtCompound;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tracing::{debug, error};
use uuid::Uuid;

struct PlayerSaveState {
    next_generation: u64,
    committed: HashMap<Uuid, u64>,
}

/// Manages the storage and retrieval of player data from disk and memory cache.
///
/// This struct provides functions to load and save player data to/from NBT files,
/// with a memory cache to handle player disconnections temporarily.
pub struct PlayerDataStorage {
    /// Path to the directory where player data is stored
    data_path: PathBuf,
    /// Whether player data saving is enabled
    save_enabled: bool,
    /// Monotonic save generations so an older snapshot cannot replace a newer one.
    save_state: Mutex<PlayerSaveState>,
}

#[derive(Debug, thiserror::Error)]
pub enum PlayerDataError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("NBT error: {0}")]
    Nbt(String),
}

impl PlayerDataStorage {
    /// Creates a new `PlayerDataStorage` with the specified data path and cache expiration time.
    pub fn new(data_path: impl Into<PathBuf>, enabled: bool) -> Self {
        let path = data_path.into();
        if !path.exists()
            && let Err(e) = fs::create_dir_all(&path)
        {
            error!(
                "Failed to create player data directory at {}: {e}",
                path.display()
            );
        }

        Self {
            data_path: path,
            save_enabled: enabled,
            save_state: Mutex::new(PlayerSaveState {
                next_generation: 0,
                committed: HashMap::new(),
            }),
        }
    }

    /// Reserves the next save generation. Allocate this when the snapshot is taken.
    pub fn allocate_save_generation(&self) -> u64 {
        let mut state = self
            .save_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.next_generation = state.next_generation.saturating_add(1);
        state.next_generation
    }

    #[must_use]
    pub const fn get_data_path(&self) -> &PathBuf {
        &self.data_path
    }

    #[must_use]
    pub const fn is_save_enabled(&self) -> bool {
        self.save_enabled
    }

    pub const fn set_save_enabled(&mut self, enabled: bool) {
        self.save_enabled = enabled;
    }

    /// Returns the path for a player's data file based on their UUID.
    #[must_use]
    pub fn get_player_data_path(&self, uuid: &Uuid) -> PathBuf {
        self.get_data_path().join(format!("{uuid}.dat"))
    }

    /// Loads player data from NBT file or cache.
    ///
    /// This function first checks if player data exists in the cache.
    /// If not, it attempts to load the data from a .dat file on disk.
    ///
    /// # Arguments
    ///
    /// * `uuid` - The UUID of the player to load data for.
    ///
    /// # Returns
    ///
    /// A Result containing either the player's NBT data or an error.
    pub fn load_player_data(&self, uuid: &Uuid) -> Result<(bool, NbtCompound), PlayerDataError> {
        // If player data saving is disabled, return empty data
        if !self.is_save_enabled() {
            return Ok((false, NbtCompound::new()));
        }

        // If not in cache, load from disk
        let path = self.get_player_data_path(uuid);
        if !path.exists() {
            debug!("No player data file found for {uuid}");
            return Ok((false, NbtCompound::new()));
        }

        let file = match File::open(&path) {
            Ok(file) => file,
            Err(e) => {
                error!("Failed to open player data file for {uuid}: {e}");
                return Err(PlayerDataError::Io(e));
            }
        };

        match pumpkin_nbt::nbt_compress::read_gzip_compound_tag(file) {
            Ok(nbt) => {
                debug!("Loaded player data for {uuid} from disk");
                Ok((true, nbt))
            }
            Err(e) => {
                error!("Failed to read player data for {uuid}: {e}");
                Err(PlayerDataError::Nbt(e.to_string()))
            }
        }
    }

    /// Saves player data to NBT file and updates cache.
    ///
    /// This function saves the player's data to a .dat file on disk and also
    /// updates the in-memory cache with the latest data.
    ///
    /// # Arguments
    ///
    /// * `uuid` - The UUID of the player to save data for.
    /// * `data` - The NBT compound data to save.
    ///
    /// # Returns
    ///
    /// A Result indicating success or the error that occurred.
    pub fn save_player_data(&self, uuid: &Uuid, data: NbtCompound) -> Result<(), PlayerDataError> {
        if !self.is_save_enabled() {
            return Ok(());
        }
        let generation = self.allocate_save_generation();
        self.save_player_data_with_generation(uuid, data, generation)
    }

    /// Saves player data for a generation allocated when the snapshot was taken.
    ///
    /// A generation older than the newest completed save for `uuid` is ignored.
    pub fn save_player_data_with_generation(
        &self,
        uuid: &Uuid,
        data: NbtCompound,
        generation: u64,
    ) -> Result<(), PlayerDataError> {
        if !self.is_save_enabled() {
            return Ok(());
        }

        let mut state = self
            .save_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state
            .committed
            .get(uuid)
            .is_some_and(|committed| *committed > generation)
        {
            debug!("Skipping stale player data save for {uuid}");
            return Ok(());
        }

        let path = self.get_player_data_path(uuid);
        if let Some(parent) = path.parent()
            && let Err(e) = fs::create_dir_all(parent)
        {
            error!("Failed to create player data directory for {uuid}: {e}");
            return Err(PlayerDataError::Io(e));
        }

        if let Err(e) = write_player_data_file(&path, data) {
            error!("Failed to write player data for {uuid}: {e}");
            return Err(e);
        }

        state.committed.insert(*uuid, generation);
        debug!("Saved player data for {uuid} to disk");
        Ok(())
    }
}

fn temporary_player_data_path(path: &Path) -> PathBuf {
    let mut temp_name = path.as_os_str().to_os_string();
    temp_name.push(".tmp");
    PathBuf::from(temp_name)
}

fn write_player_data_file(path: &Path, data: NbtCompound) -> Result<(), PlayerDataError> {
    let temp_path = temporary_player_data_path(path);
    let write_result = (|| -> Result<(), PlayerDataError> {
        let mut file = File::create(&temp_path)?;
        pumpkin_nbt::nbt_compress::write_gzip_compound_tag(data, &mut file)
            .map_err(|e| PlayerDataError::Nbt(e.to_string()))?;
        file.sync_all()?;
        fs::rename(&temp_path, path)?;
        Ok(())
    })();

    if write_result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    write_result
}

#[cfg(test)]
mod test {
    use super::{PlayerDataStorage, temporary_player_data_path};
    use crate::data::player_data::PlayerDataError;
    use pumpkin_nbt::compound::NbtCompound;
    use std::fs;
    use uuid::Uuid;

    fn named(value: &str) -> NbtCompound {
        let mut nbt = NbtCompound::new();
        nbt.put_string("name", value.to_string());
        nbt
    }

    #[test]
    fn failed_write_keeps_the_previous_player_file() {
        let dir = tempfile::tempdir().unwrap();
        let storage = PlayerDataStorage::new(dir.path(), true);
        let uuid = Uuid::new_v4();
        storage.save_player_data(&uuid, named("keep")).unwrap();

        let temp_path = temporary_player_data_path(&storage.get_player_data_path(&uuid));
        fs::create_dir(&temp_path).unwrap();

        let error = storage.save_player_data(&uuid, named("lose")).unwrap_err();
        assert!(matches!(error, PlayerDataError::Io(_)));

        let (loaded, nbt) = storage.load_player_data(&uuid).unwrap();
        assert!(loaded);
        assert_eq!(nbt.get_string("name").unwrap(), "keep");
        assert!(storage.get_player_data_path(&uuid).is_file());
    }

    #[test]
    fn older_generation_does_not_replace_a_newer_save() {
        let dir = tempfile::tempdir().unwrap();
        let storage = PlayerDataStorage::new(dir.path(), true);
        let uuid = Uuid::new_v4();
        let older = storage.allocate_save_generation();
        let newer = storage.allocate_save_generation();

        storage
            .save_player_data_with_generation(&uuid, named("new"), newer)
            .unwrap();
        storage
            .save_player_data_with_generation(&uuid, named("old"), older)
            .unwrap();

        let (loaded, nbt) = storage.load_player_data(&uuid).unwrap();
        assert!(loaded);
        assert_eq!(nbt.get_string("name").unwrap(), "new");
        assert!(!temporary_player_data_path(&storage.get_player_data_path(&uuid)).exists());
    }
}
