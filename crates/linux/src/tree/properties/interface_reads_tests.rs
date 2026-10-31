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

#[test]
fn webkit_actions_are_read_without_the_unanswered_bulk_method() {
    use zbus::connection::Builder;
    struct Action;
    #[zbus::interface(name = "org.a11y.atspi.Action")]
    impl Action {
        #[zbus(property, name = "NActions")]
        fn n_actions(&self) -> i32 {
            1
        }
        fn get_name(&self, index: i32) -> &str {
            assert_eq!(index, 0);
            "click"
        }
    }
    let (near, far) = std::os::unix::net::UnixStream::pair().unwrap();
    let (client, server) = zbus::block_on(futures_lite::future::zip(
        Builder::async_io_unix_stream(near).p2p().build(),
        Builder::async_io_unix_stream(far)
            .server(zbus::Guid::generate())
            .unwrap()
            .p2p()
            .serve_at("/node", Action)
            .unwrap()
            .build(),
    ));
    let _server = server.unwrap();
    let actions = zbus::block_on(super::read_actions(
        &client.unwrap(),
        &crate::tree::AtspiElement::new(":1.1", "/node", 1),
        &listed(&[ACTION_INTERFACE]),
        super::Liveness::Alive,
        true,
    ))
    .unwrap();
    assert_eq!(actions, listed(&["click"]));
}

struct ActionFixture {
    count: i32,
    bulk_calls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    indexed_calls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    indexed_name: String,
}

#[zbus::interface(name = "org.a11y.atspi.Action")]
impl ActionFixture {
    #[zbus(property, name = "NActions")]
    fn n_actions(&self) -> i32 {
        self.count
    }
    fn get_name(&self, _index: i32) -> zbus::fdo::Result<String> {
        self.indexed_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if self.indexed_name == "refused" {
            Err(zbus::fdo::Error::NotSupported("GetName unavailable".into()))
        } else {
            Ok(self.indexed_name.clone())
        }
    }
    fn get_actions(&self) -> Vec<(String, String, String)> {
        self.bulk_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        vec![("bulk fallback".into(), String::new(), String::new())]
    }
}

fn fixture_actions(webkit: bool, count: i32, name: &str) -> (Field<Vec<String>>, usize, usize) {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use zbus::connection::Builder;
    let bulk = Arc::new(AtomicUsize::new(0));
    let indexed = Arc::new(AtomicUsize::new(0));
    let (near, far) = std::os::unix::net::UnixStream::pair().unwrap();
    let (client, server) = zbus::block_on(futures_lite::future::zip(
        Builder::async_io_unix_stream(near).p2p().build(),
        Builder::async_io_unix_stream(far)
            .server(zbus::Guid::generate())
            .unwrap()
            .p2p()
            .serve_at(
                "/node",
                ActionFixture {
                    count,
                    bulk_calls: bulk.clone(),
                    indexed_calls: indexed.clone(),
                    indexed_name: name.to_owned(),
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
    (
        result,
        bulk.load(Ordering::SeqCst),
        indexed.load(Ordering::SeqCst),
    )
}

#[test]
fn other_toolkits_keep_bulk_fallback_and_indexed_machine_names() {
    for (name, expected) in [
        ("click", "click"),
        ("", "bulk fallback"),
        ("refused", "bulk fallback"),
    ] {
        let (actions, bulk, indexed) = fixture_actions(false, 0, name);
        assert_eq!(actions, listed(&[expected]));
        assert_eq!((bulk, indexed), (1, 1));
    }
}

#[test]
fn webkit_zero_actions_never_calls_bulk_or_indexed_methods() {
    let (actions, bulk, indexed) = fixture_actions(true, 0, "click");
    assert_eq!(actions, listed(&[]));
    assert_eq!((bulk, indexed), (0, 0));
}

#[test]
fn webkit_invalid_counts_fail_without_bulk_or_unbounded_reads() {
    for count in [-1, 1025, i32::MAX] {
        let (actions, bulk, indexed) = fixture_actions(true, count, "click");
        assert!(actions.is_failed());
        assert_eq!((bulk, indexed), (0, 0));
    }
}
