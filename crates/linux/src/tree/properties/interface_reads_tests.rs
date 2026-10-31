use super::{ACTION_INTERFACE, skipped_without};
use crate::tree::Field;
use crate::tree::properties::node_read::COMPONENT_INTERFACE;

#[test]
fn action_name_transport_loss_cannot_use_the_listed_fallback() {
    let connection = crate::tree::live::test_support::closed_connection();
    let element = crate::tree::AtspiElement::new(":1.1", "/button", 1);
    let error = zbus::block_on(super::action_name(
        &connection,
        &element,
        0,
        "click".to_owned(),
    ))
    .expect_err("transport loss must end observation");
    assert_eq!(error.code, agent_desktop_core::ErrorCode::Internal);
}

fn listed(names: &[&str]) -> Field<Vec<String>> {
    Field::Present(names.iter().map(|name| (*name).to_owned()).collect())
}

#[test]
fn an_advertised_interface_is_read() {
    let interfaces = listed(&[COMPONENT_INTERFACE]);
    assert!(skipped_without::<()>(&interfaces, COMPONENT_INTERFACE).is_none());
}

#[test]
fn an_unadvertised_interface_is_absent() {
    let interfaces = listed(&[COMPONENT_INTERFACE]);
    assert_eq!(
        skipped_without::<()>(&interfaces, ACTION_INTERFACE),
        Some(Field::Absent)
    );
}

#[test]
fn an_unreadable_interface_list_makes_the_dependent_field_a_failure() {
    let interfaces = Field::Failed("timed out".to_owned());
    let skipped = skipped_without::<()>(&interfaces, ACTION_INTERFACE);
    assert!(skipped.is_some_and(|field| field.is_failed()));
}

struct ActionFixture {
    count: i32,
    name: Option<String>,
    bulk_calls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    indexes: std::sync::Arc<std::sync::Mutex<Vec<i32>>>,
}

#[zbus::interface(name = "org.a11y.atspi.Action")]
impl ActionFixture {
    #[zbus(property, name = "NActions")]
    fn n_actions(&self) -> i32 {
        self.count
    }
    fn get_name(&self, index: i32) -> zbus::fdo::Result<String> {
        self.indexes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(index);
        self.name
            .clone()
            .ok_or_else(|| zbus::fdo::Error::NotSupported("GetName unavailable".into()))
    }
    fn get_actions(&self) -> Vec<(String, String, String)> {
        self.bulk_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        vec![("bulk fallback".into(), String::new(), String::new())]
    }
}

fn fixture_actions(
    webkit: bool,
    count: i32,
    name: Option<&str>,
) -> (Field<Vec<String>>, usize, Vec<i32>) {
    use std::sync::{Arc, Mutex, atomic::AtomicUsize};
    use zbus::connection::Builder;
    let bulk = Arc::new(AtomicUsize::new(0));
    let indexes = Arc::new(Mutex::new(Vec::new()));
    let (near, far) = std::os::unix::net::UnixStream::pair().unwrap();
    let (client, server) = zbus::block_on(futures_lite::future::zip(
        Builder::async_io_unix_stream(near)
            .p2p()
            .method_timeout(std::time::Duration::from_secs(5))
            .build(),
        Builder::async_io_unix_stream(far)
            .server(zbus::Guid::generate())
            .unwrap()
            .p2p()
            .serve_at(
                "/node",
                ActionFixture {
                    count,
                    name: name.map(str::to_owned),
                    bulk_calls: bulk.clone(),
                    indexes: indexes.clone(),
                },
            )
            .unwrap()
            .build(),
    ));
    let _server = server.unwrap();
    let result = zbus::block_on(super::read_actions(
        &client.unwrap(),
        &crate::tree::AtspiElement::new(":1.1", "/node", 1),
        &listed(&[ACTION_INTERFACE]),
        super::Liveness::Alive,
        webkit,
    ))
    .unwrap();
    let requested = indexes.lock().unwrap().clone();
    (
        result,
        bulk.load(std::sync::atomic::Ordering::SeqCst),
        requested,
    )
}

#[test]
fn webkit_actions_are_read_without_the_unanswered_bulk_method() {
    let (actions, bulk, indexes) = fixture_actions(true, 1, Some("click"));
    assert_eq!(actions, listed(&["click"]));
    assert_eq!((bulk, indexes), (0, vec![0]));
}

#[test]
fn webkit_keeps_an_answered_empty_name() {
    let (actions, bulk, _) = fixture_actions(true, 1, Some(""));
    assert_eq!(actions, listed(&[""]));
    assert_eq!(bulk, 0);
}

#[test]
fn webkit_refused_name_fails_the_field_instead_of_reading_as_empty() {
    let (actions, bulk, _) = fixture_actions(true, 1, None);
    assert!(actions.is_failed());
    assert_eq!(bulk, 0);
}

#[test]
fn other_toolkits_keep_bulk_fallback_and_indexed_machine_names() {
    for (name, expected) in [
        (Some("click"), "click"),
        (Some(""), "bulk fallback"),
        (None, "bulk fallback"),
    ] {
        let (actions, bulk, indexes) = fixture_actions(false, 0, name);
        assert_eq!(actions, listed(&[expected]));
        assert_eq!((bulk, indexes), (1, vec![0]));
    }
}

#[test]
fn webkit_zero_actions_never_calls_bulk_or_indexed_methods() {
    let (actions, bulk, indexes) = fixture_actions(true, 0, Some("click"));
    assert_eq!(actions, listed(&[]));
    assert_eq!((bulk, indexes), (0, vec![]));
}

#[test]
fn webkit_invalid_counts_fail_without_bulk_or_unbounded_reads() {
    for count in [-1, super::MAX_INDEXED_ACTIONS + 1] {
        let (actions, bulk, indexes) = fixture_actions(true, count, Some("click"));
        assert!(actions.is_failed());
        assert_eq!((bulk, indexes), (0, vec![]));
    }
}
