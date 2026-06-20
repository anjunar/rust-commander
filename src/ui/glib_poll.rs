use std::{
    sync::mpsc::{Receiver, TryRecvError},
    time::Duration,
};

use gtk::glib;

pub fn poll_receiver<T, F, D>(
    interval: Duration,
    receiver: Receiver<T>,
    mut on_event: F,
    mut on_disconnected: D,
) where
    T: 'static,
    F: FnMut(T) -> bool + 'static,
    D: FnMut() -> bool + 'static,
{
    glib::timeout_add_local(interval, move || loop {
        match receiver.try_recv() {
            Ok(event) => {
                if !on_event(event) {
                    return glib::ControlFlow::Break;
                }
            }
            Err(TryRecvError::Empty) => {
                return glib::ControlFlow::Continue;
            }
            Err(TryRecvError::Disconnected) => {
                return if on_disconnected() {
                    glib::ControlFlow::Continue
                } else {
                    glib::ControlFlow::Break
                };
            }
        }
    });
}
