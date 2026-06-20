use crate::{
    application::{load_scheduler::LoadScheduler, ActivePanel},
    application::{LoadAction, NavigationRequest},
    domain::PanelLocation,
};

#[test]
fn stale_generation_is_rejected() {
    let mut scheduler = LoadScheduler::default();
    let first = scheduler.prepare_request(request(ActivePanel::Left));
    let second = scheduler.prepare_request(request(ActivePanel::Left));

    assert!(!scheduler.commit_loaded(first.panel, first.generation));
    assert!(scheduler.commit_loaded(second.panel, second.generation));
}

#[test]
fn queued_refresh_status_is_reused_until_dirty_panels_are_drained() {
    let mut scheduler = LoadScheduler::default();
    scheduler.queue_refresh(
        &[ActivePanel::Left, ActivePanel::Right],
        "Refreshed".to_string(),
    );

    let first = scheduler.take_next_refresh("Default");
    let second = scheduler.take_next_refresh("Default");
    let third = scheduler.take_next_refresh("Default");

    assert_eq!(first, Some((ActivePanel::Left, "Refreshed".to_string())));
    assert_eq!(second, Some((ActivePanel::Right, "Refreshed".to_string())));
    assert_eq!(third, None);
}

fn request(panel: ActivePanel) -> NavigationRequest {
    NavigationRequest {
        panel,
        generation: 0,
        action: LoadAction::Navigate,
        next_location: PanelLocation::filesystem("C:/tmp".into()),
        selection_intent: None,
        status: "status".into(),
        busy_message: "busy".into(),
    }
}
