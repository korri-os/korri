//! Font lookup before a frame must not use a destroyed tree's cached window item.
//! The allocator can reuse a repeater row's address when the next component is created.

use i_slint_backend_testing::ElementHandle;
use korri_shell::backend::headless::Headless;
use slint::ComponentHandle;

#[test]
fn a_new_tree_measures_its_own_font_before_its_first_frame() {
    Headless::install();
    let source = r#"
        import "fonts/tiny5.ttf";
        export component Surface inherits Window {
            for i in 1: Text {
                text: "HOLLOW KNIGHT";
                font-family: "Pico Tiny5";
                font-size: 48px;
                width: self.preferred-width;
            }
        }
    "#;
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/packages/probe/window-item-cache.slint");
    let result = pollster::block_on(
        slint_interpreter::Compiler::default().build_from_source(source.into(), path),
    );
    assert!(
        !result.has_errors(),
        "{:?}",
        result.diagnostics().collect::<Vec<_>>()
    );
    let definition = result.component("Surface").unwrap();
    let measure = || {
        let instance = definition.create().unwrap();
        instance.show().unwrap();
        let width = ElementHandle::find_by_element_type_name(&instance, "Text")
            .next()
            .unwrap()
            .size()
            .width;
        instance.hide().unwrap();
        width
    };
    let expected = measure();
    assert!(expected > 200.0, "explicit 48px font: {expected}");
    // No frame clears the lookup cache between these lifetimes. Before the fix a
    // reused row address returned an expired window and measured the default font.
    for generation in 0..200 {
        assert_eq!(
            measure(),
            expected,
            "tree {generation}: measured another tree's font"
        );
    }
}
