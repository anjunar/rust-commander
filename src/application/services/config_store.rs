use std::{cell::RefCell, rc::Rc};

use anyhow::Result;

use crate::config::{self, AppConfig};

#[derive(Clone)]
pub struct ConfigStore {
    cache: Rc<RefCell<AppConfig>>,
}

impl ConfigStore {
    pub fn new(initial: AppConfig) -> Self {
        Self {
            cache: Rc::new(RefCell::new(initial)),
        }
    }

    pub fn cache(&self) -> Rc<RefCell<AppConfig>> {
        Rc::clone(&self.cache)
    }

    pub fn snapshot(&self) -> AppConfig {
        self.cache.borrow().clone()
    }

    pub fn replace(&self, next: AppConfig) {
        self.cache.replace(next);
    }

    pub fn save(&self, next: AppConfig) -> Result<AppConfig> {
        config::save(&next)?;
        self.replace(next.clone());
        Ok(next)
    }

    pub fn update(&self, update: impl FnOnce(&mut AppConfig)) -> Result<AppConfig> {
        let mut next = self.snapshot();
        update(&mut next);
        self.save(next)
    }
}
