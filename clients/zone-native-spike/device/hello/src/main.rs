use slint::ComponentHandle;

slint::slint! {
    import "../fonts/Inter-VariableFont.ttf";
    // Reuse the Slint spike's brand leaf/lobe/field tokens, not a new design.
    global Tokens {
        out property <color> leaf: rgb(126, 253, 59);
        out property <color> lobe: rgb(95, 214, 47);
        out property <color> field: rgb(0, 0, 0);
    }
    export component Hello inherits Window {
        title: "Zone native device proof";
        preferred-width: 640px;
        preferred-height: 480px;
        background: Tokens.lobe;
        Text {
            text: "Rust + Slint on RG35XX SP";
            font-family: "Inter";
            font-size: min(parent.width / 24, parent.height / 12);
            color: Tokens.field;
            horizontal-alignment: center;
            vertical-alignment: center;
        }
        Rectangle {
            width: min(parent.width, parent.height) / 20;
            height: self.width;
            x: (parent.width - self.width) * (0.5 + 0.4 * sin(animation-tick() / 1s * 180deg));
            y: parent.height * 0.7;
            background: Tokens.leaf;
        }
    }
}

fn main() -> Result<(), slint::PlatformError> {
    let app = Hello::new()?;
    if std::env::var("ZONE_SPIKE_FULLSCREEN").as_deref() == Ok("1") {
        app.window().set_fullscreen(true);
    }
    app.run()
}
