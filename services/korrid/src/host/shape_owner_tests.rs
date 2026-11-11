use super::*;

#[test]
fn first_connection_owns_shape_and_joiners_cannot_steal_it() {
    let mut viewers = ShapeOwners::default();
    let first = viewers.connect("fold");
    let second = viewers.connect("tv");
    assert!(viewers.is_owner(&"fold", first));
    assert!(!viewers.is_owner(&"tv", second));
    assert_eq!(viewers.owner(), Some(&"fold"));
}

#[test]
fn owner_departure_selects_longest_connected_remaining_viewer() {
    let mut viewers = ShapeOwners::default();
    let first = viewers.connect("fold");
    let second = viewers.connect("tv");
    let third = viewers.connect("tablet");
    assert!(viewers.disconnect(&"fold", first));
    assert!(viewers.is_owner(&"tv", second));
    assert!(viewers.disconnect(&"tv", second));
    assert!(viewers.is_owner(&"tablet", third));
}

#[test]
fn delayed_disconnect_from_an_old_connection_cannot_remove_its_replacement() {
    let mut viewers = ShapeOwners::default();
    let old = viewers.connect("fold");
    let current = viewers.connect("fold");
    assert!(!viewers.disconnect(&"fold", old));
    assert!(viewers.is_owner(&"fold", current));
    assert!(!viewers.is_owner(&"fold", old));
}

#[test]
fn reconnect_loses_its_place_to_viewers_that_stayed_connected() {
    let mut viewers = ShapeOwners::default();
    let old = viewers.connect("fold");
    let tv = viewers.connect("tv");
    let current = viewers.connect("fold");
    assert!(viewers.is_owner(&"tv", tv));
    assert!(!viewers.is_owner(&"fold", current));
    assert!(!viewers.disconnect(&"fold", old));
}

#[test]
fn last_disconnect_leaves_no_owner_and_next_connection_starts_again() {
    let mut viewers = ShapeOwners::default();
    let old = viewers.connect("fold");
    assert!(viewers.disconnect(&"fold", old));
    assert_eq!(viewers.owner(), None);
    let next = viewers.connect("tablet");
    assert!(viewers.is_owner(&"tablet", next));
    assert!(!viewers.disconnect(&"tablet", old));
}

#[test]
fn a_connection_token_does_not_authorize_another_device() {
    let mut viewers = ShapeOwners::default();
    let first = viewers.connect("fold");
    assert!(!viewers.disconnect(&"tv", first));
    assert!(!viewers.is_owner(&"tv", first));
    assert!(viewers.is_owner(&"fold", first));
}
