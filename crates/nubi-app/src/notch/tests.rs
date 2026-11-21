//! Compiled-surface renders and core-connected interaction checks. No desktop,
//! hooks, pipe server, settings writes or registrations are needed.
use super::*;
use image::ImageEncoder;
use nubi_core::{protocol::HookMsg, settings::Settings};
use slint::platform::software_renderer::{
    MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType,
};
use slint::platform::{Platform, PlatformError, WindowAdapter, WindowEvent};
use std::cell::Cell;

slint::slint! {
    export component TypographyProbe inherits Window {
        width: 560px; height: 148px;
        in property <bool> dark: true;
        background: dark ? #000 : #fdfcfa;
        default-font-family: "Segoe UI Variable Text";
        for weight[index] in [400, 500, 600] : Text {
            x: 12px; y: (8 + index * 28) * 1px; height: 24px;
            text: "Waiting   Sessions   Usage   DeskTime   Refresh";
            font-size: 12px; font-weight: weight;
            color: root.dark ? #fff : #1d1b17;
        }
        Text { x: 12px; y: 94px; height: 24px; text: "42%  68%  12:34  1h 24m  00:11";
            font-family: "Geist"; font-size: 16px; font-weight: 500;
            color: root.dark ? #fff : #1d1b17; }
        Text { x: 12px; y: 120px; height: 20px; text: "cargo test --offline   C:/Fig/projects/G";
            font-family: "JetBrains Mono"; font-size: 12px;
            color: root.dark ? #fff : #1d1b17; }
    }
}

#[test]
fn typography_renders_source_families_at_physical_dpi() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Offscreen(window.clone()))).unwrap();
    // Construct the production component to register its bundled font imports.
    let _notch = NotchWindow::new().unwrap();
    let ui = TypographyProbe::new().unwrap();
    ui.show().unwrap();
    let folder = std::env::var_os("NUBI_TYPE_RENDER_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/native/stage3/typography")
        });
    std::fs::create_dir_all(&folder).unwrap();
    for dark in [false, true] {
        ui.set_dark(dark);
        for percent in [100, 125, 150, 175] {
            let scale = percent as f32 / 100.;
            window.dispatch_event(WindowEvent::ScaleFactorChanged {
                scale_factor: scale,
            });
            let (width, height) = ((560. * scale) as u32, (148. * scale) as u32);
            window.set_size(slint::PhysicalSize::new(width, height));
            let mut pixels = vec![PremultipliedRgbaColor::default(); (width * height) as usize];
            window.request_redraw();
            assert!(window.draw_if_needed(|renderer| {
                renderer.render(&mut pixels, width as usize);
            }));
            let image = image::RgbImage::from_fn(width, height, |x, y| {
                let pixel = pixels[(y * width + x) as usize];
                image::Rgb([pixel.red, pixel.green, pixel.blue])
            });
            let name = format!("{}-{percent}.png", if dark { "dark" } else { "light" });
            save(&image, folder.join(name));
            // The 500/600 labels must not silently rasterize as the regular face.
            let row = |index: u32| {
                image::imageops::crop_imm(
                    &image,
                    0,
                    ((8 + index * 28) as f32 * scale) as u32,
                    width,
                    (24. * scale) as u32,
                )
                .to_image()
            };
            assert_ne!(row(0), row(1));
            assert_ne!(row(1), row(2));
        }
    }
    ui.hide().unwrap();
}

fn save(image: &image::RgbImage, path: impl AsRef<std::path::Path>) {
    let encoder = image::codecs::png::PngEncoder::new_with_quality(
        std::fs::File::create(path).unwrap(),
        image::codecs::png::CompressionType::Fast,
        image::codecs::png::FilterType::NoFilter,
    );
    encoder
        .write_image(
            image.as_raw(),
            image.width(),
            image.height(),
            image::ExtendedColorType::Rgb8,
        )
        .unwrap();
}
struct Offscreen(Rc<MinimalSoftwareWindow>);
impl Platform for Offscreen {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        Ok(self.0.clone())
    }
}

fn permission_message(provider: &str) -> HookMsg {
    HookMsg {
        provider: provider.into(),
        session_id: "permission-takeover".into(),
        event: "PermissionRequest".into(),
        tool_name: Some("Bash".into()),
        tool_input: Some(serde_json::json!({"command": "cargo test --offline"})),
        wants_reply: true,
        ..Default::default()
    }
}

/// Executes the original functions, rather than a Rust reconstruction. Requires Node.
#[test]
#[ignore = "explicit source characterization; requires Node.js"]
fn stage4_original_question_and_clock_characterization() {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let script = r#"
const assert = require('node:assert/strict');
const vm = require('node:vm');
const sources = JSON.parse(require('node:fs').readFileSync(0, 'utf8'));
class Element {
  constructor(tag, cls = '', text = '') {
    this.tag = tag; this.cls = cls; this.text = text; this.children = [];
    this.events = {}; this.style = {}; this.value = '';
    this.classList = { toggle() {} };
  }
  append(...items) { this.children.push(...items); }
  replaceChildren(...items) { this.children = items; }
  setAttribute() {}
  addEventListener(name, cb) { this.events[name] = cb; }
  focus() {}
  querySelectorAll(selector) {
    const match = e => selector === '.opt' ? e.cls === 'opt'
      : selector === 'button,input' ? ['button','input'].includes(e.tag)
      : selector === '.timebar i' ? e.tag === 'i' : e.cls === 'count';
    return this.children.flatMap(e => e instanceof Element
      ? [...(match(e) ? [e] : []), ...e.querySelectorAll(selector)] : []);
  }
  querySelector(selector) { return this.querySelectorAll(selector)[0]; }
}

for (const source of sources) {
  const calls = []; let now = 1000;
  const context = {
    el: (tag, cls, text) => new Element(tag, cls, text),
    btn: (text, cls, click) => { const e = new Element('button', cls, text); e.click = click; return e; },
    document: { createTextNode: text => text }, picks: {}, mode: () => 'full',
    sendAnswer: (id, answers) => calls.push({id, answers}),
    send: (id, answers) => calls.push({id, answers}),
    invoke: (name, args) => { calls.push({name, args}); return Promise.resolve(); },
    clock: ms => String(ms), Date: { now: () => now }, decide() {}, setTimeout() {}, Math,
  };
  vm.createContext(context);
  vm.runInContext(source.slice(source.indexOf('function questionBody('),
    source.indexOf('// Hold locally immediately;')), context);
  vm.runInContext(source.slice(source.indexOf('function requestClock('),
    source.indexOf('\nfunction ', source.indexOf('function requestClock(') + 1)), context);
  const body = new Element('div');
  context.questionBody({id: 7, session_key: 'claude:s', questions: [
    {question: 'Language?', options: [{label: 'Rust'}, {label: '日本語'}]},
    {question: 'Checks?', multiSelect: true, options: [{label: 'Build'}, {label: 'Tests'}]},
  ]}, body);
  body.querySelectorAll('.opt')[1].click();
  assert.equal(calls.length, 0); // Single click advances, without replying early.
  const done = () => body.querySelectorAll('button,input').find(e => e.text === 'Done');
  done().click(); assert.equal(calls.length, 0);
  body.querySelectorAll('.opt')[0].click();
  body.querySelectorAll('.opt')[1].click();
  body.querySelectorAll('.opt')[0].click();
  const other = body.querySelectorAll('button,input').find(e => e.tag === 'input');
  other.value = '  你好 🦀  '; other.events.input();
  other.events.keydown({key: 'Enter', isComposing: true});
  assert.equal(calls.length, 0);
  let prevented = false;
  other.events.keydown({key: 'Enter', isComposing: false, preventDefault() { prevented = true; }});
  assert.ok(prevented);
  assert.equal(JSON.stringify(calls[0]), JSON.stringify({id: 7, answers: {'Language?': '日本語', 'Checks?': 'Tests, 你好 🦀'}}));
  done().click(); assert.equal(calls.length, 1); // Only one reply.
  const blank = new Element('div');
  context.questionBody({id: 8, questions: [{question: 'Custom?', options: []}]}, blank);
  const input = blank.querySelectorAll('button,input')[0];
  input.value = '   '; input.events.input();
  input.events.keydown({key: 'Enter', isComposing: false});
  assert.equal(calls.length, 1);
  blank.querySelectorAll('button,input').find(e => e.text === 'Answer in terminal').click();
  assert.equal(JSON.stringify(calls[1]), JSON.stringify({id: 8, answers: {}}));
  blank.querySelectorAll('button,input').find(e => e.text === 'Open terminal').click();
  assert.equal(calls[2].name, 'jump_session');
  const same = new Element('div');
  context.questionBody({id: 10, questions: [{question: 'Set?', multiSelect: true, options: [{label: 'Rust'}]}]}, same);
  same.querySelectorAll('.opt')[0].click();
  const typedOption = same.querySelectorAll('button,input').find(e => e.tag === 'input');
  typedOption.value = ' Rust '; typedOption.events.input();
  same.querySelectorAll('button,input').find(e => e.text === 'Done').click();
  assert.equal(JSON.stringify(calls.at(-1)), JSON.stringify({id: 10, answers: {'Set?': 'Rust'}}));
  const card = new Element('div'); card.append(new Element('span', 'count'), new Element('i'));
  const clock = context.requestClock(card, {id: 9, created_ms: 0, expires_ms: 10000, held: false});
  card.events.pointerenter(); now += 2000; clock.a = {...clock.a, expires_ms: 12000, held: true}; clock.tick();
  assert.equal(clock.left, 9000); clock.release();
  clock.a = {...clock.a, held: false}; clock.tick();
  now += 1000; clock.tick(); assert.equal(clock.left, 8000);
  assert.equal(calls.at(-1).args.on, false);
}
"#;
    let mut child = Command::new("node")
        .args(["-e", script])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(
            serde_json::to_string(&[
                include_str!("../../../../ui/island.js"),
                include_str!("../../../../ui/overlay.js"),
            ])
            .unwrap()
            .as_bytes(),
        )
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn question_message(questions: serde_json::Value) -> HookMsg {
    HookMsg {
        event: "PreToolUse".into(),
        tool_name: Some("AskUserQuestion".into()),
        tool_input: Some(serde_json::json!({"questions": questions})),
        deadline_secs: nubi_core::protocol::QUESTION_DEADLINE_SECS,
        ..permission_message("claude")
    }
}

#[test]
fn stage4_questions_advance_once_and_send_original_hook_payload_on_every_edge() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Offscreen(window))).unwrap();
    for edge in ["top", "bottom", "left", "right"] {
        for mode in ["system", "full"] {
            let (host, _events) = NativeHost::from_settings(Settings {
                edge: edge.into(),
                questions_mode: mode.into(),
                follow_quiet_time: false,
                ..Default::default()
            });
            let ui = NotchWindow::new().unwrap();
            let notch = Notch::connect(&ui, host.clone());
            let message = question_message(serde_json::json!([
                {"question":"Language?", "header":"Language", "options":[{"label":"Rust"},{"label":"日本語"}]},
                {"question":"Checks?", "multiSelect":true, "options":[{"label":"Build"},{"label":"Tests"}]}
            ]));
            with(&host, |state| {
                state.on_event(&message);
            });
            let (tx, mut rx) = tokio::sync::oneshot::channel();
            let id = host.add_pending(&message, tx, 60_000);
            notch.borrow_mut().update(&ui);
            let data = ui.global::<NotchData>();
            let key: slint::SharedString = id.to_string().into();
            assert_eq!(ui.get_tab(), 0);
            assert_eq!(notch.borrow_mut().view(&ui).0, 3);
            data.invoke_choose(key.clone(), -1);
            data.invoke_choose(key.clone(), 99);
            assert_eq!(data.get_requests().row_data(0).unwrap().question_index, 0);
            data.invoke_choose(key.clone(), 1);
            let row = data.get_requests().row_data(0).unwrap();
            assert_eq!(row.question_index, 1);
            assert!(row.multi && !row.can_submit && !row.slim);
            assert!(row.step.starts_with("2 of 2"));
            data.invoke_submit(key.clone());
            assert!(rx.try_recv().is_err());
            data.invoke_choose(key.clone(), 0);
            let options = data.get_requests().row_data(0).unwrap().options;
            data.invoke_choose(key.clone(), 1);
            data.invoke_choose(key.clone(), 0);
            assert_eq!(options, data.get_requests().row_data(0).unwrap().options);
            assert!(!options.row_data(0).unwrap().selected);
            assert!(options.row_data(1).unwrap().selected);
            notch.borrow_mut().update(&ui);
            data.invoke_submit(key.clone());
            let reply = rx.try_recv().expect("Done answers the hook waiter");
            assert_eq!(reply.decision, "answer");
            assert!(reply.message.is_none());
            assert_eq!(reply.answers.as_ref().unwrap()["Language?"], "日本語");
            assert_eq!(reply.answers.as_ref().unwrap()["Checks?"], "Tests");
            let output: serde_json::Value = serde_json::from_str(
                &nubi_core::protocol::question_json(message.tool_input.as_ref().unwrap(), &reply)
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(output["hookSpecificOutput"]["permissionDecision"], "allow");
            assert_eq!(
                output["hookSpecificOutput"]["updatedInput"]["questions"],
                message.tool_input.as_ref().unwrap()["questions"]
            );
            assert_eq!(data.get_pending_count(), 0);
            assert!(notch.borrow().drafts.is_empty());
            assert!(with(&host, |state| !state.sessions
                [&format!("{}:{}", message.provider, message.session_id)]
                .attention));
            data.invoke_submit(key);
            assert_eq!(data.get_requests().row_count(), 0);
        }
    }
}

#[test]
fn stage4_custom_answers_match_source_set_and_enter_semantics() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Offscreen(window))).unwrap();
    for multi in [false, true] {
        let (host, _events) = NativeHost::from_settings(Settings::default());
        let ui = NotchWindow::new().unwrap();
        let notch = Notch::connect(&ui, host.clone());
        let message = question_message(serde_json::json!([
            {"question":"Custom?", "multiSelect":multi, "options":[{"label":"Rust"},{"label":"Tests"}]}
        ]));
        let (tx, mut rx) = tokio::sync::oneshot::channel();
        let id = host.add_pending(&message, tx, 60_000);
        notch.borrow_mut().update(&ui);
        let data = ui.global::<NotchData>();
        let key: slint::SharedString = id.to_string().into();
        if multi {
            data.invoke_choose(key.clone(), 0);
            data.invoke_custom(key.clone(), " Rust ".into());
            assert_eq!(notch.borrow().drafts[&id].answers["Custom?"], vec!["Rust"]);
            data.invoke_choose(key.clone(), 1);
        }
        data.invoke_custom(key.clone(), "old custom".into());
        data.invoke_custom(key.clone(), "   ".into());
        data.invoke_submit_custom(key.clone(), "   ".into());
        assert!(
            rx.try_recv().is_err(),
            "blank Enter never submits selected options"
        );
        data.invoke_custom(key.clone(), "  你好 🦀  ".into());
        for _ in 0..3 {
            notch.borrow_mut().update(&ui);
        }
        assert_eq!(
            data.get_requests().row_data(0).unwrap().custom,
            "  你好 🦀  "
        );
        ui.set_tab(1);
        ui.invoke_tab_selected(1);
        ui.set_tab(0);
        ui.invoke_tab_selected(0);
        // Enter must reapply the current input, even if the last edit callback had older text.
        data.invoke_submit_custom(key, "  日本語 🦀  ".into());
        let reply = rx.try_recv().unwrap();
        assert_eq!(
            reply.answers.unwrap()["Custom?"],
            if multi {
                "Rust, Tests, 日本語 🦀"
            } else {
                "日本語 🦀"
            }
        );
        assert!(data.get_request_error().is_empty());
    }
}

#[test]
fn stage4_question_drafts_survive_card_removal_and_provider_refresh() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Offscreen(window))).unwrap();
    let (host, _events) = NativeHost::from_settings(Settings::default());
    let ui = NotchWindow::new().unwrap();
    let notch = Notch::connect(&ui, host.clone());
    let (tx, _rx) = tokio::sync::oneshot::channel();
    let first = host.add_pending(&permission_message("codex"), tx, 60_000);
    let (tx, mut rx) = tokio::sync::oneshot::channel();
    let id = host.add_pending(
        &question_message(serde_json::json!([
            {"question":"Draft?", "multiSelect":true, "options":[{"label":"Rust"}]}
        ])),
        tx,
        60_000,
    );
    notch.borrow_mut().update(&ui);
    let data = ui.global::<NotchData>();
    let key: slint::SharedString = id.to_string().into();
    data.invoke_choose(key.clone(), 0);
    data.invoke_custom(key.clone(), "  draft 🦀  ".into());
    let options = data.get_requests().row_data(1).unwrap().options;
    host.remove_pending(first, false);
    notch.borrow_mut().update(&ui);
    let row = data.get_requests().row_data(0).unwrap();
    assert_eq!(row.id, key);
    assert_eq!(row.options, options);
    assert_eq!(row.custom, "  draft 🦀  ");
    data.invoke_hold(key.clone(), true);
    data.set_focused_request(key.clone());
    data.set_input_focused(true);
    with(&host, |state| state.settings.providers = "codex".into());
    notch.borrow_mut().update(&ui);
    assert_eq!(data.get_requests().row_count(), 0);
    assert!(!with(&host, |state| state.pending[&id].held));
    assert!(data.get_focused_request().is_empty());
    assert!(!data.get_input_focused());
    with(&host, |state| state.settings.providers = "both".into());
    notch.borrow_mut().update(&ui);
    assert_eq!(data.get_requests().row_data(0).unwrap().custom, row.custom);
    data.invoke_submit(key);
    assert_eq!(
        rx.try_recv().unwrap().answers.unwrap()["Draft?"],
        "Rust, draft 🦀"
    );
}

#[test]
fn stage4_question_terminal_hold_deadline_and_missed_lifecycle() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Offscreen(window))).unwrap();
    let (host, _events) = NativeHost::from_settings(Settings {
        questions_mode: "full".into(),
        follow_quiet_time: false,
        ..Default::default()
    });
    let ui = NotchWindow::new().unwrap();
    let notch = Notch::connect(&ui, host.clone());
    let message = question_message(serde_json::json!([{"question":"Waiting?", "options":[]}]));
    with(&host, |state| {
        state.on_event(&message);
    });
    let (tx, mut rx) = tokio::sync::oneshot::channel();
    let id = host.add_pending(&message, tx, 60_000);
    notch.borrow_mut().update(&ui);
    let data = ui.global::<NotchData>();
    let key: slint::SharedString = id.to_string().into();
    data.invoke_custom(key.clone(), "draft".into());
    data.invoke_hold(key.clone(), true);
    notch.borrow_mut().update(&ui);
    let before = data.get_requests().row_data(0).unwrap();
    with(&host, |state| {
        state.pending.get_mut(&id).unwrap().expires_ms -= 5000
    });
    notch.borrow_mut().update(&ui);
    let after = data.get_requests().row_data(0).unwrap();
    assert_eq!(before.countdown, after.countdown);
    assert_eq!(before.remaining, after.remaining);
    data.invoke_hold(key.clone(), false);
    notch.borrow_mut().update(&ui);
    assert!(data.get_requests().row_data(0).unwrap().remaining < before.remaining);
    data.invoke_terminal(format!("{}:{}", message.provider, message.session_id).into());
    assert_eq!(data.get_request_error(), "Terminal window not found");
    assert!(
        rx.try_recv().is_err(),
        "Open terminal does not answer or discard drafts"
    );
    assert_eq!(notch.borrow().drafts[&id].custom, "draft");
    data.invoke_answer(key, "skip".into());
    let reply = rx.try_recv().unwrap();
    assert_eq!(reply.decision, "skip");
    assert!(reply.answers.as_ref().unwrap().is_empty());
    assert!(
        nubi_core::protocol::question_json(message.tool_input.as_ref().unwrap(), &reply).is_none()
    );
    assert!(with(&host, |state| state.missed.is_empty()));
    assert!(data.get_request_error().is_empty());

    let (tx, mut rx) = tokio::sync::oneshot::channel();
    let id = host.add_pending(&message, tx, 60_000);
    notch.borrow_mut().update(&ui);
    let key: slint::SharedString = id.to_string().into();
    data.invoke_custom(key.clone(), "expired draft".into());
    data.invoke_hold(key.clone(), true);
    let hard_ms = now_ms().saturating_sub(1);
    with(&host, |state| {
        state.pending.get_mut(&id).unwrap().expires_ms = hard_ms - 100
    });
    assert!(
        host.pending_tick(id, 250, hard_ms),
        "holds cannot outlive the hook deadline"
    );
    assert_eq!(with(&host, |state| state.pending[&id].expires_ms), hard_ms);
    host.remove_pending(id, true);
    notch.borrow_mut().update(&ui);
    assert!(notch.borrow().drafts.is_empty());
    assert_eq!(data.get_pending_count(), 0);
    assert_eq!(data.get_missed_count(), 1);
    let missed = data.get_requests().row_data(0).unwrap();
    assert!(missed.missed && !missed.question);
    assert_eq!(missed.title, "Question waiting in terminal");
    assert!(rx.try_recv().is_err());
    data.invoke_terminal("".into());
    assert_eq!(data.get_request_error(), "Terminal window not found");
    assert_eq!(data.get_missed_count(), 1);
    with(&host, |state| {
        state.on_event(&HookMsg {
            event: "PostToolUse".into(),
            ..message
        });
    });
    notch.borrow_mut().update(&ui);
    assert_eq!(data.get_missed_count(), 0);
    assert_eq!(data.get_requests().row_count(), 0);
}

#[test]
fn stage4_question_pointer_and_keyboard_survive_step_and_card_removal() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Offscreen(window.clone()))).unwrap();
    let (host, _events) = NativeHost::from_settings(Settings::default());
    let ui = NotchWindow::new().unwrap();
    let notch = Notch::connect(&ui, host.clone());
    ui.set_ready(true);
    ui.set_view_mode(3);
    ui.set_opened(true);
    ui.set_along(620.);
    ui.set_depth(440.);
    ui.set_open_height(440.);
    ui.set_tab(0);
    ui.show().unwrap();
    window.set_size(slint::PhysicalSize::new(720, 700));
    with(&host, |state| {
        state.settings.questions_mode = "system".into()
    });
    let (tx, _rx) = tokio::sync::oneshot::channel();
    let earlier = host.add_pending(&permission_message("codex"), tx, 60_000);
    let message = question_message(serde_json::json!([
        {"question":"Language?", "options":[{"label":"Rust"}]},
        {"question":"Custom?", "options":[]}
    ]));
    let (tx, mut rx) = tokio::sync::oneshot::channel();
    let id = host.add_pending(&message, tx, 60_000);
    notch.borrow_mut().update(&ui);
    let data = ui.global::<NotchData>();
    let settle = || {
        for _ in 0..3 {
            let mut pixels = vec![PremultipliedRgbaColor::default(); 720 * 700];
            window.request_redraw();
            window.draw_if_needed(|renderer| {
                renderer.render(&mut pixels, 720);
            });
            slint::platform::update_timers_and_animations();
        }
    };
    settle();
    let position = slint::LogicalPosition::new(140., 216.);
    window.dispatch_event(WindowEvent::PointerMoved { position });
    window.dispatch_event(WindowEvent::PointerPressed {
        position,
        button: slint::platform::PointerEventButton::Left,
    });
    window.dispatch_event(WindowEvent::PointerReleased {
        position,
        button: slint::platform::PointerEventButton::Left,
    });
    assert_eq!(data.get_requests().row_data(1).unwrap().question_index, 1);
    // Run the layout/frame work that delivers changed bindings, without a desktop window.
    settle();
    assert!(data.get_input_focused());
    window.dispatch_event(WindowEvent::KeyPressed {
        text: "  你好 🦀  ".into(),
    });
    window.dispatch_event(WindowEvent::KeyReleased {
        text: "  你好 🦀  ".into(),
    });
    for _ in 0..3 {
        notch.borrow_mut().update(&ui);
    }
    assert_eq!(
        data.get_requests().row_data(1).unwrap().custom,
        "  你好 🦀  "
    );
    host.remove_pending(earlier, false);
    notch.borrow_mut().update(&ui);
    settle();
    assert_eq!(
        data.get_requests().row_data(0).unwrap().custom,
        "  你好 🦀  "
    );
    window.dispatch_event(WindowEvent::KeyPressed { text: "続".into() });
    window.dispatch_event(WindowEvent::KeyReleased { text: "続".into() });
    window.dispatch_event(WindowEvent::KeyPressed { text: "\n".into() });
    window.dispatch_event(WindowEvent::KeyReleased { text: "\n".into() });
    let reply = rx
        .try_recv()
        .expect("real TextInput accepted callback answers the hook");
    assert_eq!(reply.answers.unwrap()["Custom?"], "你好 🦀  続");
    assert!(!notch.borrow().drafts.contains_key(&id));
    assert!(!data.get_input_focused());
    ui.hide().unwrap();
}

#[test]
fn stage4_question_tab_activation_and_scroll_on_every_edge() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Offscreen(window.clone()))).unwrap();
    for (edge, dark, scale) in (0..4).flat_map(|edge| {
        [false, true].into_iter().flat_map(move |dark| {
            [1., 1.25, 1.5, 1.75]
                .into_iter()
                .map(move |scale| (edge, dark, scale))
        })
    }) {
        let (host, _events) = NativeHost::from_settings(Settings {
            edge: ["top", "bottom", "left", "right"][edge as usize].into(),
            notch_theme: if dark { "dark" } else { "light" }.into(),
            reduce_motion: true,
            questions_mode: "full".into(),
            ..Default::default()
        });
        let ui = NotchWindow::new().unwrap();
        let notch = Notch::connect(&ui, host.clone());
        ui.set_ready(true);
        ui.set_view_mode(3);
        ui.set_opened(true);
        ui.set_edge(edge);
        ui.set_along(if edge >= 2 { 440. } else { 620. });
        ui.set_depth(if edge >= 2 { 360. } else { 440. });
        ui.set_open_height(440.);
        ui.set_tab(0);
        ui.show().unwrap();
        window.dispatch_event(WindowEvent::ScaleFactorChanged {
            scale_factor: scale,
        });
        ui.global::<Theme>().set_scale(scale);
        let (width, height) = ((720. * scale) as u32, (700. * scale) as u32);
        window.set_size(slint::PhysicalSize::new(width, height));
        let options: Vec<_> = (0..12).map(|index| serde_json::json!({
            "label": format!("Choice {index}"),
            "description": "A long description that wraps on narrow question cards and exercises variable option heights."
        })).collect();
        let message = question_message(serde_json::json!([{
            "question":"Choose checks using the keyboard", "multiSelect":true, "options":options
        }]));
        with(&host, |state| {
            state.on_event(&message);
        });
        let (tx, mut rx) = tokio::sync::oneshot::channel();
        let id = host.add_pending(&message, tx, 600_000);
        notch.borrow_mut().update(&ui);
        let data = ui.global::<NotchData>();
        let settle = || {
            let mut pixels = vec![PremultipliedRgbaColor::default(); (width * height) as usize];
            window.request_redraw();
            window.draw_if_needed(|renderer| {
                renderer.render(&mut pixels, width as usize);
            });
            slint::platform::update_timers_and_animations();
        };
        let key = |text: slint::SharedString| {
            window.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
            window.dispatch_event(WindowEvent::KeyReleased { text });
            settle();
        };
        settle();
        assert!(ui.get_requests_content_height() > ui.get_open_height());
        let viewport_top = if edge >= 2 {
            130.
        } else if edge == 1 {
            260.
        } else {
            0.
        } + 52.;
        let viewport_bottom = viewport_top + 372.;
        let viewport_x = if edge == 2 {
            180.
        } else if edge == 3 {
            540.
        } else {
            360.
        };
        let scroll_position = slint::LogicalPosition::new(viewport_x, viewport_top + 100.);
        window.dispatch_event(WindowEvent::PointerMoved {
            position: scroll_position,
        });
        window.dispatch_event(WindowEvent::PointerScrolled {
            position: scroll_position,
            delta_x: 0.,
            delta_y: -10_000.,
        });
        settle();
        // At the lower scroll limit the last option is just above the input/footer.
        let position = slint::LogicalPosition::new(
            viewport_x,
            viewport_bottom - 120. - if edge >= 2 { 36. } else { 0. },
        );
        for selected in [true, false] {
            window.dispatch_event(WindowEvent::PointerMoved { position });
            window.dispatch_event(WindowEvent::PointerPressed {
                position,
                button: slint::platform::PointerEventButton::Left,
            });
            window.dispatch_event(WindowEvent::PointerReleased {
                position,
                button: slint::platform::PointerEventButton::Left,
            });
            settle();
            assert_eq!(
                data.get_requests()
                    .row_data(0)
                    .unwrap()
                    .options
                    .row_data(11)
                    .unwrap()
                    .selected,
                selected,
                "wheel must expose the last option, edge={edge}, dark={dark}, scale={scale}"
            );
        }
        window.dispatch_event(WindowEvent::PointerScrolled {
            position: scroll_position,
            delta_x: 0.,
            delta_y: 10_000.,
        });
        settle();
        ui.invoke_focus_requests();
        // Traverse the real header scopes; no direct choose/submit callback invocation.
        for _ in 0..20 {
            key(slint::platform::Key::Tab.into());
            if data.get_focused_request() == id.to_string() {
                break;
            }
        }
        assert_eq!(data.get_focused_request(), id.to_string(), "edge={edge}");
        let first_y = data.get_focus_y();
        let mut last_y = first_y;
        for index in 0..12 {
            if index > 0 {
                key(slint::platform::Key::Tab.into());
            }
            key(if index % 2 == 0 { " " } else { "\n" }.into());
            let row = data.get_requests().row_data(0).unwrap();
            assert!(
                row.options.row_data(index).unwrap().selected,
                "edge={edge}, option={index}"
            );
            assert_eq!(
                row.options.iter().filter(|option| option.selected).count(),
                index + 1
            );
            last_y = data.get_focus_y();
        }
        // Offscreen options must cause focus scrolling, rather than continuing below the viewport.
        assert!(
            last_y < first_y + ui.get_open_height(),
            "edge={edge}, first={first_y}, last={last_y}"
        );
        key(slint::platform::Key::Tab.into());
        assert!(
            data.get_input_focused(),
            "Other follows the last option, edge={edge}"
        );
        window.dispatch_event(WindowEvent::KeyPressed {
            text: slint::platform::Key::Shift.into(),
        });
        key(slint::platform::Key::Tab.into());
        window.dispatch_event(WindowEvent::KeyReleased {
            text: slint::platform::Key::Shift.into(),
        });
        assert!(
            !data.get_input_focused(),
            "Shift+Tab returns to last option, edge={edge}"
        );
        key(" ".into());
        assert!(
            !data
                .get_requests()
                .row_data(0)
                .unwrap()
                .options
                .row_data(11)
                .unwrap()
                .selected
        );
        key(slint::platform::Key::Tab.into());
        assert!(data.get_input_focused());
        key("Unicode 日本語 🦀".into());
        assert!(rx.try_recv().is_err());
        key(slint::platform::Key::Tab.into());
        assert!(!data.get_input_focused());
        // Open terminal must keep the hook pending when no real host is bound.
        key(slint::platform::Key::Tab.into());
        key(slint::platform::Key::Tab.into());
        key("\n".into());
        assert_eq!(data.get_request_error(), "Terminal window not found");
        assert_eq!(
            with(&host, |state| state.sessions["claude:permission-takeover"]
                .action_error
                .clone()),
            Some("Terminal window not found".into())
        );
        assert!(rx.try_recv().is_err());
        window.dispatch_event(WindowEvent::KeyPressed {
            text: slint::platform::Key::Shift.into(),
        });
        key(slint::platform::Key::Tab.into());
        key(slint::platform::Key::Tab.into());
        window.dispatch_event(WindowEvent::KeyReleased {
            text: slint::platform::Key::Shift.into(),
        });
        key("\n".into());
        let reply = rx
            .try_recv()
            .expect("Done is reachable after the input and submits");
        let expected = (0..11)
            .map(|i| format!("Choice {i}"))
            .chain(["Unicode 日本語 🦀".into()])
            .collect::<Vec<_>>()
            .join(", ");
        assert_eq!(
            reply.answers.unwrap()["Choose checks using the keyboard"],
            expected
        );
        eprintln!(
            "question traversal/activation/scroll: edge={edge}, dark={dark}, scale={scale}: pass"
        );
        ui.hide().unwrap();
    }
}

#[test]
fn stage4_automatic_requests_stay_open_when_pointer_enters() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Offscreen(window))).unwrap();
    for edge in ["top", "bottom", "left", "right"] {
        for fold in [false, true] {
            for question in [false, true] {
                let (host, _events) = NativeHost::from_settings(Settings {
                    edge: edge.into(),
                    notch_on_hover: fold,
                    request_popup_secs: 2,
                    follow_quiet_time: false,
                    ..Default::default()
                });
                let ui = NotchWindow::new().unwrap();
                let notch = Notch::connect(&ui, host.clone());
                let mut message = permission_message("claude");
                if question {
                    message.event = "PreToolUse".into();
                    message.tool_name = Some("AskUserQuestion".into());
                    message.tool_input = Some(serde_json::json!({"questions":[{
                        "question":"Which?", "options":[{"label":"Rust"}]
                    }]}));
                }
                let (tx, mut rx) = tokio::sync::oneshot::channel();
                let id = host.add_pending(&message, tx, 60_000);
                notch.borrow_mut().update(&ui);
                assert_eq!(notch.borrow_mut().view(&ui).0, 3);
                notch.borrow_mut().pointer(true, &ui);
                assert_eq!(notch.borrow_mut().view(&ui).0, 3,
                    "pointerenter must retain automatic open: {edge}, fold={fold}, question={question}");
                notch.borrow_mut().popup = Some(Instant::now() - Duration::from_secs(3));
                assert_eq!(
                    notch.borrow_mut().view(&ui).0,
                    3,
                    "hover protects popup expiry"
                );
                if question {
                    ui.global::<NotchData>()
                        .invoke_choose(id.to_string().into(), 0);
                    assert_eq!(rx.try_recv().unwrap().answers.unwrap()["Which?"], "Rust");
                } else {
                    ui.global::<NotchData>()
                        .invoke_answer(id.to_string().into(), "allow".into());
                    assert_eq!(rx.try_recv().unwrap().decision, "allow");
                }
                assert!(with(&host, |state| state.pending.is_empty()));
                notch.borrow_mut().pointer(false, &ui);
                assert!(notch.borrow().popup.is_none());
            }
        }
    }
}

#[test]
fn stage4_permissions_open_without_hover_and_reply_on_every_edge() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Offscreen(window))).unwrap();
    for edge in ["top", "bottom", "left", "right"] {
        for fold in [false, true] {
            for provider in ["claude", "codex"] {
                let (host, _events) = NativeHost::from_settings(Settings {
                    edge: edge.into(),
                    notch_on_hover: fold,
                    follow_quiet_time: false,
                    ..Default::default()
                });
                let ui = NotchWindow::new().unwrap();
                let notch = Notch::connect(&ui, host.clone());
                notch.borrow_mut().fold_since = Instant::now() - Duration::from_secs(1);
                assert_eq!(notch.borrow_mut().view(&ui).0, if fold { 0 } else { 1 });
                let message = permission_message(provider);
                with(&host, |state| {
                    state.on_event(&message);
                });
                for decision in ["allow", "deny"] {
                    ui.set_tab(2);
                    let (tx, mut rx) = tokio::sync::oneshot::channel();
                    let id = host.add_pending(&message, tx, 60_000);
                    notch.borrow_mut().update(&ui);
                    assert!(!notch.borrow().hovering);
                    assert_eq!(ui.get_tab(), 0, "{edge}, fold={fold}, {provider}");
                    assert_eq!(notch.borrow_mut().view(&ui).0, 3, "{edge}, fold={fold}");
                    let data = ui.global::<NotchData>();
                    assert_eq!(data.get_pending_count(), 1);
                    let row = data.get_requests().row_data(0).unwrap();
                    assert_eq!(row.id, id.to_string());
                    assert!(!row.question && !row.missed);
                    assert_eq!(row.side, matches!(edge, "left" | "right"));
                    data.invoke_answer(id.to_string().into(), decision.into());
                    let reply = rx.try_recv().expect("decision reaches the hook waiter");
                    assert_eq!(reply.decision, decision);
                    assert_eq!(
                        reply.message.as_deref(),
                        (decision == "deny").then_some("Denied from Nubi")
                    );
                    assert!(reply.answers.is_none());
                    assert!(with(&host, |state| state.pending.is_empty()));
                    assert_eq!(data.get_pending_count(), 0);
                    assert_eq!(data.get_requests().row_count(), 0);
                    assert!(notch.borrow().popup.is_none());
                    assert!(data.get_request_error().is_empty());
                }
            }
        }
    }
}

#[test]
fn stage4_permissions_preserve_dnd_and_provider_suppression_on_every_edge() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Offscreen(window))).unwrap();
    for edge in ["top", "bottom", "left", "right"] {
        for provider in ["claude", "codex"] {
            for dnd in [false, true] {
                let (host, _events) = NativeHost::from_settings(Settings {
                    edge: edge.into(),
                    providers: if dnd {
                        "both"
                    } else if provider == "claude" {
                        "codex"
                    } else {
                        "claude"
                    }
                    .into(),
                    follow_quiet_time: false,
                    ..Default::default()
                });
                with(&host, |state| state.dnd_manual = dnd);
                let ui = NotchWindow::new().unwrap();
                let notch = Notch::connect(&ui, host.clone());
                ui.set_tab(2);
                let (tx, mut rx) = tokio::sync::oneshot::channel();
                let id = host.add_pending(&permission_message(provider), tx, 60_000);
                notch.borrow_mut().update(&ui);
                assert_eq!(ui.get_tab(), 2);
                assert_ne!(notch.borrow_mut().view(&ui).0, 3, "{edge}, dnd={dnd}");
                let data = ui.global::<NotchData>();
                assert_eq!(data.get_dnd(), dnd);
                assert_eq!(data.get_pending_count(), i32::from(dnd));
                assert_eq!(data.get_requests().row_count(), usize::from(dnd));
                assert!(rx.try_recv().is_err());
                assert!(with(&host, |state| state.pending.contains_key(&id)));
                if dnd {
                    with(&host, |state| state.dnd_manual = false);
                    notch.borrow_mut().update(&ui);
                    assert!(
                        notch.borrow().popup.is_none(),
                        "DND backlog does not auto-open"
                    );
                } else {
                    with(&host, |state| state.settings.providers = "both".into());
                    notch.borrow_mut().update(&ui);
                    assert_eq!(notch.borrow_mut().view(&ui).0, 3);
                }
                host.remove_pending(id, false);
            }
        }
    }
}

#[test]
fn stage4_permissions_preserve_popup_hold_timeout_and_retry_errors() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Offscreen(window))).unwrap();
    for edge in ["top", "bottom", "left", "right"] {
        let (host, _events) = NativeHost::from_settings(Settings {
            edge: edge.into(),
            request_popup_secs: 2,
            follow_quiet_time: false,
            ..Default::default()
        });
        let ui = NotchWindow::new().unwrap();
        let notch = Notch::connect(&ui, host.clone());
        let (tx, mut rx) = tokio::sync::oneshot::channel();
        let id = host.add_pending(&permission_message("claude"), tx, 60_000);
        notch.borrow_mut().update(&ui);
        notch.borrow_mut().popup = Some(Instant::now() - Duration::from_secs(3));
        assert_eq!(
            notch.borrow_mut().view(&ui).0,
            1,
            "popup duration is separate from takeover"
        );
        assert!(rx.try_recv().is_err());
        assert!(with(&host, |state| state.pending.contains_key(&id)));
        with(&host, |state| state.settings.request_popup_secs = 0);
        notch.borrow_mut().popup = Some(Instant::now() - Duration::from_secs(3600));
        assert_eq!(
            notch.borrow_mut().view(&ui).0,
            3,
            "zero keeps requests open"
        );
        let data = ui.global::<NotchData>();
        let expiry = with(&host, |state| state.pending[&id].expires_ms);
        data.invoke_hold(id.to_string().into(), true);
        host.pending_tick(id, 250, expiry + 100);
        assert_eq!(
            with(&host, |state| state.pending[&id].expires_ms),
            expiry + 100
        );
        host.pending_tick(id, 250, expiry + 100);
        assert_eq!(
            with(&host, |state| state.pending[&id].expires_ms),
            expiry + 100
        );
        notch.borrow_mut().pointer(false, &ui);
        assert!(!with(&host, |state| state.pending[&id].held));
        host.pending_tick(id, 250, expiry + 1000);
        assert_eq!(
            with(&host, |state| state.pending[&id].expires_ms),
            expiry + 100
        );
        host.remove_pending(id, true);
        notch.borrow_mut().update(&ui);
        assert_eq!(data.get_pending_count(), 0);
        assert_eq!(data.get_missed_count(), 1);
        assert!(data.get_requests().row_data(0).unwrap().missed);
        data.invoke_answer(id.to_string().into(), "allow".into());
        assert_eq!(
            data.get_request_error(),
            "request expired or already answered"
        );
        assert!(rx.try_recv().is_err());
        let (tx, rx) = tokio::sync::oneshot::channel();
        let gone = host.add_pending(&permission_message("claude"), tx, 60_000);
        drop(rx);
        data.invoke_answer(gone.to_string().into(), "allow".into());
        assert_eq!(data.get_request_error(), "hook already gone");
        let (tx, mut rx) = tokio::sync::oneshot::channel();
        let retry = host.add_pending(&permission_message("claude"), tx, 60_000);
        notch.borrow_mut().update(&ui);
        data.invoke_answer(retry.to_string().into(), "deny".into());
        assert_eq!(rx.try_recv().unwrap().decision, "deny");
        assert!(data.get_request_error().is_empty());
        assert!(with(&host, |state| state.pending.is_empty()));
    }
}

#[test]
fn tabs_and_toolbar_deliver_the_first_click() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Offscreen(window.clone()))).unwrap();
    let ui = NotchWindow::new().unwrap();
    ui.set_ready(true);
    ui.set_view_mode(3);
    ui.set_opened(true);
    ui.set_open_height(240.);
    ui.global::<NotchData>().set_desk_visible(true);
    ui.show().unwrap();
    let click = |x, y| {
        let position = slint::LogicalPosition::new(x, y);
        window.dispatch_event(WindowEvent::PointerMoved { position });
        window.dispatch_event(WindowEvent::PointerPressed {
            position,
            button: slint::platform::PointerEventButton::Left,
        });
        window.dispatch_event(WindowEvent::PointerReleased {
            position,
            button: slint::platform::PointerEventButton::Left,
        });
    };
    let actions = Rc::new(Cell::new(0));
    let received = actions.clone();
    ui.on_toggle_dnd(move || received.set(received.get() + 1));
    let panels = Rc::new(Cell::new(0));
    let received = panels.clone();
    ui.on_open_panel(move || received.set(received.get() + 1));
    let terminals = Rc::new(RefCell::new(Vec::new()));
    let received = terminals.clone();
    ui.global::<NotchData>()
        .on_terminal(move |key| received.borrow_mut().push(key.to_string()));
    ui.global::<NotchData>()
        .set_sessions(model(vec![NotchSession {
            key: "fixture".into(),
            name: "Fixture session".into(),
            ..Default::default()
        }]));
    for scale in [1., 1.25, 1.5, 1.75] {
        window.dispatch_event(WindowEvent::ScaleFactorChanged {
            scale_factor: scale,
        });
        ui.global::<Theme>().set_scale(scale);
        for edge in 0..4 {
            ui.set_edge(edge);
            let side = edge >= 2;
            let (width, height) = if side { (420., 860.) } else { (720., 700.) };
            window.set_size(slint::PhysicalSize::new(
                (width * scale) as u32,
                (height * scale) as u32,
            ));
            ui.set_along(if side { 240. } else { 620. });
            ui.set_depth(if side { 360. } else { 240. });
            // Reproduce the populated four-tab header that previously clipped Open panel.
            ui.global::<NotchData>()
                .set_session_count(if side { 1 } else { 0 });
            ui.global::<NotchData>().set_requests(model(if side {
                vec![NotchRequest {
                    id: "waiting".into(),
                    ..Default::default()
                }]
            } else {
                vec![]
            }));
            let x = if side {
                if edge == 2 {
                    12.
                } else {
                    72.
                }
            } else {
                62.
            };
            let y = match edge {
                0 => 0.,
                1 => 460.,
                _ => 310.,
            };
            ui.set_tab(1);
            for (tab, offset) in [(2, 175.), (0, 33.), (3, 248.), (1, 108.)] {
                click(
                    x + offset - if side { tab as f32 * 4. } else { 0. },
                    y + 28.,
                );
                assert_eq!(
                    ui.get_tab(),
                    tab,
                    "first click: edge {edge}, DPI {scale}, tab {tab}"
                );
            }
            let before = actions.get();
            click(x + if side { 299. } else { 550. }, y + 28.);
            assert_eq!(
                actions.get(),
                before + 1,
                "toolbar first click: edge {edge}, DPI {scale}"
            );
            let before = panels.get();
            click(x + if side { 325. } else { 582. }, y + 28.);
            assert_eq!(
                panels.get(),
                before + 1,
                "panel button first click: edge {edge}, DPI {scale}"
            );
            let before = terminals.borrow().len();
            click(x + 50., y + 72.);
            assert_eq!(
                terminals.borrow().len(),
                before + 1,
                "session first click: edge {edge}, DPI {scale}"
            );
            assert_eq!(terminals.borrow().last().unwrap(), "fixture");
        }
    }
    ui.hide().unwrap();
}

#[test]
fn request_controls_deliver_the_first_click() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Offscreen(window.clone()))).unwrap();
    let ui = NotchWindow::new().unwrap();
    ui.set_ready(true);
    ui.set_view_mode(3);
    ui.set_along(620.);
    ui.set_depth(440.);
    ui.set_open_height(440.);
    ui.set_tab(0);
    ui.show().unwrap();
    window.set_size(slint::PhysicalSize::new(720, 700));
    let click = |x, y| {
        let position = slint::LogicalPosition::new(x, y);
        window.dispatch_event(WindowEvent::PointerMoved { position });
        window.dispatch_event(WindowEvent::PointerPressed {
            position,
            button: slint::platform::PointerEventButton::Left,
        });
        window.dispatch_event(WindowEvent::PointerReleased {
            position,
            button: slint::platform::PointerEventButton::Left,
        });
    };
    let decision = Rc::new(RefCell::new(String::new()));
    let received = decision.clone();
    ui.global::<NotchData>()
        .on_answer(move |id, answer| *received.borrow_mut() = format!("{id}:{answer}"));
    ui.global::<NotchData>()
        .set_requests(model(vec![NotchRequest {
            id: "fixture".into(),
            title: "Run command?".into(),
            summary: "cargo test".into(),
            height: 146.,
            ..Default::default()
        }]));
    click(173., 172.);
    assert_eq!(*decision.borrow(), "fixture:deny");
    let choice = Rc::new(Cell::new(-1));
    let received = choice.clone();
    ui.global::<NotchData>().on_choose(move |id, index| {
        assert_eq!(id, "fixture");
        received.set(index);
    });
    ui.global::<NotchData>()
        .set_requests(model(vec![NotchRequest {
            id: "fixture".into(),
            question: true,
            prompt: "Which language?".into(),
            options: model(vec![crate::RequestOption {
                label: "Rust".into(),
                ..Default::default()
            }]),
            height: 280.,
            ..Default::default()
        }]));
    ui.invoke_focus_requests();
    click(140., 140.);
    assert_eq!(choice.get(), 0);
    ui.hide().unwrap();
}
fn capture(window: &MinimalSoftwareWindow, ui: &NotchWindow, name: &str, scale: f32, side: bool) {
    window.dispatch_event(WindowEvent::ScaleFactorChanged {
        scale_factor: scale,
    });
    ui.global::<Theme>().set_scale(scale);
    let (w, h) = if side { (420., 860.) } else { (720., 700.) };
    let (width, height) = ((w * scale) as u32, (h * scale) as u32);
    window.set_size(slint::PhysicalSize::new(width, height));
    let mut pixels = vec![PremultipliedRgbaColor::default(); width as usize * height as usize];
    window.request_redraw();
    assert!(window.draw_if_needed(|renderer| {
        renderer.render(&mut pixels, width as usize);
    }));
    let mut image = image::RgbImage::new(width, height);
    let mut opaque = 0;
    for (index, pixel) in pixels.iter().enumerate() {
        if pixel.alpha == 255 {
            opaque += 1;
        }
        let background = ((255 - u16::from(pixel.alpha)) * 96 / 255) as u8;
        image.put_pixel(
            index as u32 % width,
            index as u32 / width,
            image::Rgb([
                pixel.red.saturating_add(background),
                pixel.green.saturating_add(background),
                pixel.blue.saturating_add(background),
            ]),
        );
    }
    assert!(
        opaque > (if ui.get_view_mode() == 0 { 100. } else { 1000. } * scale * scale) as usize,
        "empty rendered notch: {name}"
    );
    // The software renderer's buffer has the target physical size, never a
    // resized 100% bitmap. Separate nearest-neighbor zooms expose AA and borders.
    let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/native/stage3/renders");
    std::fs::create_dir_all(&folder).unwrap();
    save(&image, folder.join(format!("{name}.png")));
    if !name.contains("-125-top-usage")
        && !name.contains("-175-right-sessions")
        && !name.contains("-125-top-question")
    {
        return;
    }
    let crop = if side {
        image::imageops::crop_imm(
            &image,
            0,
            (height / 2).saturating_sub((ui.get_along() * scale / 2.) as u32 + 24),
            width,
            (ui.get_along() * scale + 48.).min(height as f32) as u32,
        )
        .to_image()
    } else {
        let y = if ui.get_edge() == 1 {
            height.saturating_sub((ui.get_depth() * scale + 24.) as u32)
        } else {
            0
        };
        image::imageops::crop_imm(
            &image,
            0,
            y,
            width,
            (ui.get_depth() * scale + 24.).min(height as f32) as u32,
        )
        .to_image()
    };
    let zoom = image::imageops::resize(
        &crop,
        crop.width() * 3,
        crop.height() * 3,
        image::imageops::FilterType::Nearest,
    );
    save(&zoom, folder.join(format!("{name}-zoom.png")));
}
#[test]
fn live_notch_decisions_folding_edges_and_physical_renders() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Offscreen(window.clone()))).unwrap();
    let settings = Settings {
        desktime_tab: true,
        desktime_enabled: true,
        ..Default::default()
    };
    let (host, _events) = NativeHost::from_settings(settings);
    let stamp = now_ms();
    with(&host, |state| {
        for (provider, name) in [("claude", "Nubi · native notch"), ("codex", "Stage 3")] {
            state.on_event(&HookMsg {
                provider: provider.into(),
                session_id: "render".into(),
                event: "SessionStart".into(),
                ..Default::default()
            });
            let session = state
                .sessions
                .get_mut(&format!("{provider}:render"))
                .unwrap();
            session.name = name.into();
            session.state = if provider == "claude" {
                "working"
            } else {
                "done"
            }
            .into();
            session.context_tokens = Some(48000);
            state.usage.insert(
                provider.into(),
                nubi_core::state::Usage {
                    provider: provider.into(),
                    status: "ok".into(),
                    fetched_ms: Some(stamp),
                    message: None,
                    windows: vec![
                        nubi_core::state::Window {
                            id: "5h".into(),
                            label: "5h".into(),
                            used_percent: 42.,
                            resets_at: Some((stamp / 1000) as i64 + 3660),
                        },
                        nubi_core::state::Window {
                            id: "7d".into(),
                            label: "7d".into(),
                            used_percent: 68.,
                            resets_at: Some((stamp / 1000) as i64 + 7200),
                        },
                    ],
                },
            );
        }
    });
    let ui = NotchWindow::new().unwrap();
    let notch = Notch::connect(&ui, host.clone());
    ui.set_ready(true);
    ui.show().unwrap();
    let message = HookMsg {
        provider: "claude".into(),
        session_id: "render".into(),
        event: "PermissionRequest".into(),
        tool_name: Some("Bash".into()),
        tool_input: Some(serde_json::json!({"command":"cargo test --offline --workspace"})),
        ..Default::default()
    };
    let (tx, mut rx) = tokio::sync::oneshot::channel();
    let id = host.add_pending(&message, tx, 600000);
    notch.borrow_mut().update(&ui);
    assert_eq!(ui.get_tab(), 0);
    assert_eq!(notch.borrow_mut().view(&ui).0, 3);
    ui.global::<NotchData>()
        .invoke_hold(id.to_string().into(), true);
    assert!(with(&host, |state| state.pending[&id].held));
    ui.set_view_mode(3);
    ui.set_opened(true);
    ui.set_along(620.);
    ui.set_depth(112.);
    ui.set_open_height(112.);
    window.dispatch_event(WindowEvent::PointerMoved {
        position: slint::LogicalPosition::new(520., 74.),
    });
    assert!(
        with(&host, |state| state.pending[&id].held),
        "request hover includes Allow/Deny buttons"
    );
    let expiry = with(&host, |state| state.pending[&id].expires_ms);
    host.pending_tick(id, 250, expiry + 1000);
    assert_eq!(
        with(&host, |state| state.pending[&id].expires_ms),
        expiry + 250
    );
    ui.global::<NotchData>()
        .invoke_answer(id.to_string().into(), "deny".into());
    let decision = rx.try_recv().unwrap();
    assert_eq!(decision.decision, "deny");
    assert_eq!(decision.message.as_deref(), Some("Denied from Nubi"));
    let question = HookMsg {
        tool_name: Some("AskUserQuestion".into()),
        event: "PreToolUse".into(),
        tool_input: Some(serde_json::json!({"questions":[
        {"question":"Which language?","options":[{"label":"Rust","description":"Native runtime"},{"label":"日本語"}],"multiSelect":false},
        {"question":"Which checks?","options":[{"label":"Build"},{"label":"Tests"}],"multiSelect":true}]})),
        ..message.clone()
    };
    let (tx, mut answers) = tokio::sync::oneshot::channel();
    let qid = host.add_pending(&question, tx, 600000);
    notch.borrow_mut().update(&ui);
    ui.global::<NotchData>()
        .invoke_choose(qid.to_string().into(), 1);
    assert!(
        ui.global::<NotchData>()
            .get_requests()
            .row_data(0)
            .unwrap()
            .multi
    );
    ui.global::<NotchData>()
        .invoke_submit(qid.to_string().into());
    assert!(answers.try_recv().is_err());
    ui.global::<NotchData>()
        .invoke_choose(qid.to_string().into(), 0);
    ui.global::<NotchData>()
        .invoke_custom(qid.to_string().into(), "  Unicode ✓  ".into());
    ui.set_tab(1);
    ui.invoke_tab_selected(1);
    ui.set_tab(0);
    ui.invoke_tab_selected(0);
    notch.borrow_mut().update(&ui);
    assert_eq!(
        ui.global::<NotchData>()
            .get_requests()
            .row_data(0)
            .unwrap()
            .custom,
        "  Unicode ✓  "
    );
    ui.global::<NotchData>()
        .set_focused_request(qid.to_string().into());
    ui.global::<NotchData>().set_input_focused(true);
    ui.global::<NotchData>()
        .invoke_submit(qid.to_string().into());
    assert!(ui.global::<NotchData>().get_focused_request().is_empty());
    assert!(!ui.global::<NotchData>().get_input_focused());
    let reply = answers.try_recv().unwrap().answers.unwrap();
    assert_eq!(reply["Which language?"], "日本語");
    assert_eq!(reply["Which checks?"], "Build, Unicode ✓");
    // Real keyboard events exercise tab focus and confirmation trapping.
    ui.set_tab(0);
    ui.invoke_focus_requests();
    window.dispatch_event(WindowEvent::KeyPressed {
        text: slint::platform::Key::Tab.into(),
    });
    window.dispatch_event(WindowEvent::KeyReleased {
        text: slint::platform::Key::Tab.into(),
    });
    window.dispatch_event(WindowEvent::KeyPressed {
        text: slint::platform::Key::RightArrow.into(),
    });
    assert_eq!(ui.get_tab(), 1);
    window.dispatch_event(WindowEvent::KeyPressed {
        text: slint::platform::Key::End.into(),
    });
    assert_eq!(ui.get_tab(), 3);
    ui.global::<NotchData>()
        .invoke_session_menu("claude:render".into());
    window.dispatch_event(WindowEvent::KeyPressed {
        text: slint::platform::Key::End.into(),
    });
    assert_eq!(ui.get_menu_index(), 3);
    window.dispatch_event(WindowEvent::KeyPressed { text: "\n".into() });
    assert_eq!(ui.get_confirmation(), 1);
    window.dispatch_event(WindowEvent::KeyPressed {
        text: slint::platform::Key::Tab.into(),
    });
    assert_eq!(ui.get_menu_index(), 1);
    window.dispatch_event(WindowEvent::KeyPressed {
        text: slint::platform::Key::Escape.into(),
    });
    assert!(ui.get_menu_key().is_empty());
    ui.set_tab(0);
    let (tx, _rx) = tokio::sync::oneshot::channel();
    let missed = host.add_pending(&message, tx, 600000);
    host.remove_pending(missed, true);
    notch.borrow_mut().update(&ui);
    assert!(
        ui.global::<NotchData>()
            .get_requests()
            .row_data(0)
            .unwrap()
            .missed
    );
    ui.global::<NotchData>()
        .invoke_dismiss(missed.to_string().into());
    assert!(with(&host, |state| state.missed.is_empty()));
    let (tx, _rx) = tokio::sync::oneshot::channel();
    let hidden = host.add_pending(
        &HookMsg {
            provider: "codex".into(),
            ..message.clone()
        },
        tx,
        600000,
    );
    host.remove_pending(hidden, true);
    let (tx, _rx) = tokio::sync::oneshot::channel();
    let visible = host.add_pending(&message, tx, 600000);
    host.remove_pending(visible, true);
    with(&host, |state| state.settings.providers = "claude".into());
    notch.borrow_mut().update(&ui);
    assert_eq!(ui.global::<NotchData>().get_missed_count(), 1);
    ui.global::<NotchData>().invoke_dismiss("".into());
    assert_eq!(
        with(&host, |state| state
            .missed
            .iter()
            .map(|item| item.id)
            .collect::<Vec<_>>()),
        vec![hidden]
    );
    with(&host, |state| {
        state.settings.providers = "both".into();
        state.missed.clear();
    });
    use chrono::Datelike;
    let today = chrono::Local::now().date_naive();
    let monday = today - chrono::Duration::days(today.weekday().num_days_from_monday().into());
    let month_start = today.with_day(1).unwrap();
    let month_end = (month_start + chrono::Duration::days(32))
        .with_day(1)
        .unwrap()
        - chrono::Duration::days(1);
    for (view, start, end, productive) in [
        ("day", today, today, 310),
        ("week", monday, monday + chrono::Duration::days(6), 1530),
        ("month", month_start, month_end, 4000),
    ] {
        host.desktime.lock().unwrap().reports.insert(
            view.into(),
            (
                nubi_core::desktime::Report {
                    view: view.into(),
                    productive_min: Some(productive),
                    at_work_min: Some(productive + 40),
                    range_start: start.to_string(),
                    range_end: end.to_string(),
                    holidays: vec![],
                    logged_out: false,
                },
                stamp as i64,
            ),
        );
    }
    let mut renders = 0;
    for theme in ["dark", "light"] {
        with(&host, |state| state.settings.notch_theme = theme.into());
        for percent in [100, 125, 150, 175] {
            let scale = percent as f32 / 100.;
            for edge in ["top", "bottom", "left", "right"] {
                with(&host, |state| state.settings.edge = edge.into());
                notch.borrow_mut().update(&ui);
                assert_eq!(ui.global::<Theme>().get_dark(), theme == "dark");
                for (state, tab, view) in [
                    ("compact", 1, 1),
                    ("sessions", 1, 3),
                    ("usage", 2, 3),
                    ("desktime", 3, 3),
                    ("folded", 1, 0),
                ] {
                    ui.set_tab(tab);
                    notch.borrow_mut().hovering = view == 3;
                    notch.borrow_mut().clicked = view == 3;
                    let (_, mut values) = notch.borrow_mut().view(&ui);
                    if view == 0 {
                        values = [56., 6., 3., 4.];
                    }
                    if view == 1 {
                        values = if ui.get_edge() >= 2 {
                            [122., 46., 18., 8.]
                        } else {
                            [145., 44., 16., 8.]
                        };
                    }
                    ui.set_view_mode(view);
                    ui.set_opened(view == 3);
                    ui.set_along(values[0] as f32);
                    ui.set_depth(values[1] as f32);
                    ui.set_corner(values[2] as f32);
                    ui.set_ear(values[3] as f32);
                    capture(
                        &window,
                        &ui,
                        &format!("{theme}-{percent}-{edge}-{state}"),
                        scale,
                        ui.get_edge() >= 2,
                    );
                    renders += 1;
                }
            }
            with(&host, |state| {
                state.settings.edge = "top".into();
                state.settings.questions_mode = "full".into();
            });
            for (state, message) in [
                ("approval", message.clone()),
                ("question", question.clone()),
                ("slim", message.clone()),
            ] {
                with(&host, |core| {
                    core.settings.questions_mode =
                        if state == "slim" { "system" } else { "full" }.into()
                });
                let (tx, _rx) = tokio::sync::oneshot::channel();
                let id = host.add_pending(&message, tx, 600000);
                notch.borrow_mut().update(&ui);
                ui.set_tab(0);
                let (_, values) = notch.borrow_mut().view(&ui);
                ui.set_view_mode(3);
                ui.set_opened(true);
                ui.set_along(values[0] as f32);
                ui.set_depth(values[1] as f32);
                ui.set_corner(values[2] as f32);
                ui.set_ear(values[3] as f32);
                capture(
                    &window,
                    &ui,
                    &format!("{theme}-{percent}-top-{state}"),
                    scale,
                    false,
                );
                renders += 1;
                host.remove_pending(id, false);
            }
        }
    }
    assert_eq!(renders, 184);
    // All edges keep permission and missed-card actions inside the narrow card.
    for edge in ["left", "right", "bottom"] {
        with(&host, |state| state.settings.edge = edge.into());
        for (name, msg, mode) in [
            ("slim", message.clone(), "system"),
            ("question", question.clone(), "full"),
        ] {
            with(&host, |state| state.settings.questions_mode = mode.into());
            let (tx, _rx) = tokio::sync::oneshot::channel();
            let id = host.add_pending(&msg, tx, 600000);
            notch.borrow_mut().update(&ui);
            ui.set_tab(0);
            let (_, values) = notch.borrow_mut().view(&ui);
            ui.set_view_mode(3);
            ui.set_opened(true);
            ui.set_along(values[0] as f32);
            ui.set_depth(values[1] as f32);
            capture(
                &window,
                &ui,
                &format!("light-125-{edge}-{name}"),
                1.25,
                ui.get_side(),
            );
            host.remove_pending(id, false);
        }
    }
    // DeskTime errors never leak into Usage or survive a successful refresh.
    ui.set_tab(3);
    host.desktime.lock().unwrap().error = Some("offline".into());
    notch.borrow_mut().update(&ui);
    assert!(ui
        .global::<NotchData>()
        .get_refresh_error()
        .starts_with("Couldn't refresh"));
    ui.set_tab(2);
    ui.invoke_tab_selected(2);
    assert!(ui.global::<NotchData>().get_refresh_error().is_empty());
    ui.set_tab(3);
    host.desktime.lock().unwrap().error = None;
    notch.borrow_mut().update(&ui);
    assert!(ui.global::<NotchData>().get_refresh_error().is_empty());
    for (enabled, logged_out) in [(false, false), (true, true)] {
        with(&host, |state| state.settings.desktime_enabled = enabled);
        host.desktime.lock().unwrap().logged_out = logged_out;
        notch.borrow_mut().update(&ui);
        assert_eq!(ui.global::<NotchData>().get_desk_sections().row_count(), 0);
    }
    with(&host, |state| {
        state.settings.notch_accent = "blue".into();
        state.settings.reduce_motion = true;
    });
    notch.borrow_mut().update(&ui);
    assert_eq!(ui.global::<Theme>().get_notch_accent(), "blue");
    assert!(ui.global::<Theme>().get_reduced());
    with(&host, |state| {
        state.settings.notch_on_hover = true;
        state.pending.clear();
        state.missed.clear();
    });
    notch.borrow_mut().update(&ui);
    notch.borrow_mut().pointer(false, &ui);
    notch.borrow_mut().fold_since = Instant::now() - Duration::from_millis(451);
    assert_eq!(notch.borrow_mut().view(&ui).0, 0);
    notch.borrow_mut().pointer(true, &ui);
    assert_eq!(notch.borrow_mut().view(&ui).0, 1);
    notch.borrow_mut().clicked = true;
    assert_eq!(notch.borrow_mut().view(&ui).0, 3);
    ui.hide().unwrap();
}

/// Uses the exact browser fixture without starting provider loops or native services.
#[test]
fn source_fixture_typography_and_dimensions() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../../../spikes/notch-slint/fixture.json")).unwrap();
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Offscreen(window.clone()))).unwrap();
    let ui = NotchWindow::new().unwrap();
    let (host, _) = NativeHost::from_settings(Settings::default());
    let notch = Notch::connect(&ui, host.clone());
    let stamp = fixture["now"].as_u64().unwrap();
    with(&host, |state| {
        // Capture the compact view independently of PNG encoding speed. The
        // production hover setting folds it after 450 ms; folding is tested above.
        state.settings.notch_on_hover = false;
        for usage in fixture["usage"].as_array().unwrap() {
            let provider = usage["provider"].as_str().unwrap();
            state.usage.insert(
                provider.into(),
                nubi_core::state::Usage {
                    provider: provider.into(),
                    status: "ok".into(),
                    fetched_ms: Some(stamp),
                    message: None,
                    windows: usage["windows"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|w| nubi_core::state::Window {
                            id: w["label"].as_str().unwrap().into(),
                            label: w["label"].as_str().unwrap().into(),
                            used_percent: w["used_percent"].as_f64().unwrap(),
                            resets_at: w["resets_at"].as_i64(),
                        })
                        .collect(),
                },
            );
        }
    });
    sync(
        &notch.borrow().providers,
        with(&host, |state| {
            ["claude", "codex"]
                .iter()
                .map(|p| models::provider_row(state, p, stamp))
                .collect()
        }),
    );
    sync(
        &notch.borrow().sessions,
        fixture["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| {
                let state = s["state"].as_str().unwrap();
                NotchSession {
                    key: s["key"].as_str().unwrap().into(),
                    name: s["name"].as_str().unwrap().into(),
                    provider: s["provider"].as_str().unwrap().into(),
                    state: if state == "done" { "Done" } else { "Idle" }.into(),
                    context: format!("{}k", s["context_tokens"].as_u64().unwrap() / 1000).into(),
                    ago: format!(
                        "{}m",
                        (stamp - s["last_event_ms"].as_u64().unwrap()) / 60000
                    )
                    .into(),
                    ..Default::default()
                }
            })
            .collect(),
    );
    ui.global::<NotchData>().set_session_count(3);
    ui.global::<NotchData>().set_desk_visible(true);
    ui.on_tab_geometry(|x, width| println!("source selected tab x={x} width={width}"));
    ui.set_ready(true);
    ui.show().unwrap();
    for theme in ["dark", "light"] {
        ui.global::<Theme>().set_dark(theme == "dark");
        with(&host, |state| state.settings.notch_theme = theme.into());
        for percent in [100, 125, 150, 175] {
            let scale = percent as f32 / 100.;
            window.dispatch_event(WindowEvent::ScaleFactorChanged {
                scale_factor: scale,
            });
            ui.global::<Theme>().set_scale(scale);
            window.set_size(slint::PhysicalSize::new(
                (720. * scale) as u32,
                (700. * scale) as u32,
            ));
            for (name, mode, tab, height) in [
                ("closed", 1, 1, 44.),
                ("sessions", 3, 1, 188.),
                ("usage", 3, 2, 180.),
            ] {
                ui.set_view_mode(mode);
                ui.set_tab(tab);
                ui.set_opened(mode == 3);
                notch.borrow_mut().hovering = mode == 3;
                notch.borrow_mut().clicked = mode == 3;
                let (actual_mode, dimensions) = notch.borrow_mut().view(&ui);
                assert_eq!(actual_mode, mode);
                assert_eq!(dimensions[1], height);
                ui.set_along(dimensions[0] as f32);
                ui.set_depth(dimensions[1] as f32);
                ui.set_corner(dimensions[2] as f32);
                ui.set_ear(dimensions[3] as f32);
                capture(
                    &window,
                    &ui,
                    &format!("source-{theme}-{name}-{percent}"),
                    scale,
                    false,
                );
                println!(
                    "source {name} {percent}%: {} x {} logical pixels",
                    ui.get_along(),
                    ui.get_depth()
                );
                assert_eq!(
                    ui.get_compact_width(),
                    145.,
                    "two source percentages must match DOM offsetWidth"
                );
            }
            // A third digit must grow the closed notch; fixed provider slots hid this.
            let providers = ui.global::<NotchData>().get_providers();
            let mut first = providers.row_data(0).unwrap();
            first.text = "100%".into();
            providers.set_row_data(0, first.clone());
            assert_eq!(ui.get_compact_width(), 151.);
            first.text = "32%".into();
            providers.set_row_data(0, first);
            // Use the same populated source fixture for narrow side-edge rows.
            for edge in [2, 3] {
                ui.set_edge(edge);
                ui.set_view_mode(3);
                ui.set_tab(1);
                ui.set_opened(true);
                window.set_size(slint::PhysicalSize::new(
                    (420. * scale) as u32,
                    (860. * scale) as u32,
                ));
                let (_, dimensions) = notch.borrow_mut().view(&ui);
                assert_eq!(dimensions[0], 188.);
                assert_eq!(dimensions[1], 360.);
                ui.set_along(dimensions[0] as f32);
                ui.set_depth(dimensions[1] as f32);
                ui.set_corner(dimensions[2] as f32);
                ui.set_ear(dimensions[3] as f32);
                capture(
                    &window,
                    &ui,
                    &format!("source-{theme}-side{edge}-sessions-{percent}"),
                    scale,
                    true,
                );
                if theme == "dark" && percent == 100 && edge == 2 {
                    let sessions = ui.global::<NotchData>().get_sessions();
                    let originals: Vec<_> = (0..2)
                        .map(|index| sessions.row_data(index).unwrap())
                        .collect();
                    for (index, original) in originals.iter().enumerate() {
                        let mut row = original.clone();
                        row.working = true;
                        row.state = "Working".into();
                        sessions.set_row_data(index, row);
                    }
                    ui.set_equalizer_phase(0.25);
                    capture(&window, &ui, "source-dark-side2-working-100", scale, true);
                    for (index, row) in originals.into_iter().enumerate() {
                        sessions.set_row_data(index, row);
                    }
                    let originals: Vec<_> = (0..3)
                        .map(|index| sessions.row_data(index).unwrap())
                        .collect();
                    let names = [
                        "Long project name with many words that should end with an ellipsis",
                        "Another very long session name for the active agent",
                        "Long project name with queued compaction and multiple subagents",
                    ];
                    let states = [
                        "Done",
                        "Working · 3 agents",
                        "Working · 12 agents · Compact queued",
                    ];
                    for (index, original) in originals.iter().enumerate() {
                        let mut row = original.clone();
                        row.name = names[index].into();
                        row.state = states[index].into();
                        row.working = index > 0;
                        sessions.set_row_data(index, row);
                    }
                    capture(&window, &ui, "source-dark-side2-overflow-100", scale, true);
                    for (index, row) in originals.into_iter().enumerate() {
                        sessions.set_row_data(index, row);
                    }
                }
            }
            ui.set_edge(0);
        }
    }
    ui.hide().unwrap();
}

#[test]
fn compact_percentage_rounding_matches_original_javascript() {
    let mut state = Inner::default();
    for (percent, expected) in [
        (0.0, "0%"),
        (0.49, "0%"),
        (0.5, "1%"),
        (2.5, "3%"),
        (42.49, "42%"),
        (42.5, "43%"),
        (42.51, "43%"),
        (98.5, "99%"),
        (99.5, "100%"),
        (100.0, "100%"),
    ] {
        for provider in ["claude", "codex"] {
            state.usage.insert(
                provider.into(),
                nubi_core::state::Usage {
                    provider: provider.into(),
                    status: "ok".into(),
                    windows: vec![nubi_core::state::Window {
                        label: "5h".into(),
                        used_percent: percent,
                        ..Default::default()
                    }],
                    ..Default::default()
                },
            );
            let row = provider_row(&state, provider, 0);
            assert_eq!(row.text.as_str(), expected, "{provider}: {percent}");
            assert_eq!(row.session, percent as f32);
        }
    }
}
