use std::process::Command;

#[test]
fn trace_child() {
    if std::env::var_os("TRACE_CHILD").is_none() {
        return;
    }
    korri_input_trace::presented();
    korri_input_trace::expect_frame();
    korri_input_trace::presented();
    korri_input_trace::presented();
    for n in 0..350 {
        korri_input_trace::record("sample", format_args!("n={n}"));
    }
}

#[test]
fn trace_is_opt_in_capped_and_only_marks_the_next_frame() {
    for setting in [None, Some("0"), Some("1")] {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", "trace_child", "--nocapture"])
            .env("TRACE_CHILD", "1")
            .env_remove("KORRI_INPUT_TRACE");
        if let Some(value) = setting {
            command.env("KORRI_INPUT_TRACE", value);
        }
        let output = command.output().unwrap();
        assert!(output.status.success());
        let stderr = String::from_utf8(output.stderr).unwrap();
        let lines: Vec<_> = stderr.lines().collect();
        assert_eq!(lines.len(), if setting == Some("1") { 300 } else { 0 });
        if setting == Some("1") {
            assert!(lines[0].contains("stage=frame_presented"));
            let times: Vec<u64> = lines
                .iter()
                .map(|line| {
                    line.split("mono_us=")
                        .nth(1)
                        .unwrap()
                        .split_whitespace()
                        .next()
                        .unwrap()
                        .parse()
                        .unwrap()
                })
                .collect();
            assert!(times.windows(2).all(|t| t[1] >= t[0]));
            assert!(lines[1].contains("stage=sample n=0"));
        }
    }
}
