use super::ReadPurpose;

#[test]
fn shape_and_gate_read_the_child_list_and_none_of_the_evidence() {
    for purpose in [ReadPurpose::Shape, ReadPurpose::Gate] {
        assert!(purpose.children());
        assert!(!purpose.identity());
        assert!(!purpose.vocabulary());
        assert!(!purpose.value());
        assert!(!purpose.bounds());
        assert!(!purpose.actions());
    }
}

#[test]
fn every_purpose_that_reads_actions_reads_the_attributes_that_route_them() {
    use ReadPurpose::*;
    for purpose in [
        Snapshot, Sampling, Identity, Resolve, Element, State, Bounds, Value, Actions, Frame,
        Structure, Shape, Gate,
    ] {
        assert!(!purpose.actions() || purpose.vocabulary());
    }
}
