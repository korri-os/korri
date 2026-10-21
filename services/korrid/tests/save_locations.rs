use korrid::launcher::plugin_launch::{evaluate, PluginLaunchInput};

fn input(root: &std::path::Path) -> PluginLaunchInput {
    serde_json::from_value(serde_json::json!({
        "runnerId":"@korri:libretro/mgba", "program":"retroarch",
        "accountRoot":root, "saveRoot":root.join("saves/game"),
        "cacheRoot":root.join("cache"), "files":{}, "artifacts":{}
    }))
    .unwrap()
}

#[test]
fn runner_declares_save_locations_inside_its_game_save_root() {
    let root = tempfile::tempdir().unwrap();
    let input = input(root.path());
    let plan = evaluate("export const name = 'runner'; export const handlers = {'launch.prepare': input => ({command:input.program, args:[], saveLocations:[{key:'saves',kind:'save',path:input.saveRoot+'/saves'}]})}", &input).unwrap();
    let wire = serde_json::to_value(plan).unwrap();
    assert_eq!(wire["saveLocations"][0]["key"], "saves");
}

#[test]
fn runner_cannot_declare_an_escape_or_an_unknown_save_kind() {
    let root = tempfile::tempdir().unwrap();
    let input = input(root.path());
    for location in [
        "{key:'saves',kind:'save',path:input.saveRoot+'/../../other'}",
        "{key:'saves',kind:'save',path:'/etc'}",
        "{key:'saves',kind:'other',path:input.saveRoot+'/saves'}",
    ] {
        let source = format!("export const name = 'runner'; export const handlers = {{'launch.prepare': input => ({{command:input.program, args:[], saveLocations:[{location}]}})}}");
        assert!(evaluate(&source, &input).is_err(), "{location}");
    }
}

#[test]
fn runner_cannot_escape_through_a_symlink() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("saves/game")).unwrap();
    std::os::unix::fs::symlink(root.path(), root.path().join("saves/game/escape")).unwrap();
    let source = "export const name = 'runner'; export const handlers = {'launch.prepare': input => ({command:input.program,args:[],saveLocations:[{key:'saves',kind:'save',path:input.saveRoot+'/escape/new'}]})}";
    assert!(evaluate(source, &input(root.path())).is_err());
}
