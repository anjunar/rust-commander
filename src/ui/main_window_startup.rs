use std::rc::Rc;

pub struct StartupLoadState {
    pub wait_for_initial_panels: bool,
    pub left_done: bool,
    pub right_done: bool,
    pub on_ready: Option<Rc<dyn Fn()>>,
}

impl StartupLoadState {
    pub fn new(wait_for_initial_panels: bool) -> Self {
        Self {
            wait_for_initial_panels,
            left_done: false,
            right_done: false,
            on_ready: None,
        }
    }

    pub fn is_complete(&self) -> bool {
        self.left_done && self.right_done
    }
}
