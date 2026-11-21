//! Existing core services, dispatched by the panel without a WebView.
use crate::{
    host::NativeHost,
    notch::Notch,
    panel_logic::Panel,
    panel_settings::{model, text},
    NotchWindow, PanelData, PanelField, PanelRelease, ParityPanel,
};
use nubi_core::host::{with, Host};
use serde_json::{json, Map, Value};
use slint::ComponentHandle;
use std::{cell::RefCell, rc::Rc};
type Register = Rc<dyn Fn(&nubi_core::settings::Settings) -> Result<(), String>>;
#[derive(Clone)]
pub struct Services {
    pub host: NativeHost,
    pub runtime: tokio::runtime::Handle,
    pub register: Register,
    pub notch: Rc<RefCell<Notch>>,
    pub notch_ui: slint::Weak<NotchWindow>,
}
pub fn connect(panel: &ParityPanel, owner: Rc<RefCell<Panel>>, services: Services) {
    let weak = panel.as_weak();
    panel.global::<PanelData>().on_action(move |action,key| {
        let Some(panel)=weak.upgrade() else{return;};
        let data=panel.global::<PanelData>();
        if action == "reveal-focus" {
            let reporting=data.get_report_open();
            let top=if reporting {44.}else{72.};
            let bottom=panel.window().size().to_logical(panel.window().scale_factor()).height - if reporting {108.}else if panel.get_dirty(){156.}else{96.};
            let y=data.get_focus_y();
            let change=if y<top {top-y}else if y+48.>bottom {bottom-y-48.}else{0.};
            if reporting {data.set_report_y((data.get_report_y()+change).min(0.));}else{data.set_settings_y((data.get_settings_y()+change).min(0.));}
            return;
        }
        let result=match action.as_str() {
            "copy"|"copy-session-path"=>{
                let text=if action=="copy-session-path" {session_path(&services.host,&key).unwrap_or_default()}else{key.to_string()};
                slint::private_unstable_api::re_exports::WindowInner::from_pub(panel.window()).context().platform().set_clipboard_text(&text,slint::platform::Clipboard::DefaultClipboard);
                Ok(())
            },
            "dnd"=>{services.host.toggle_dnd();Ok(())},
            "refresh"=>{owner.borrow_mut().start_refresh(&services.host);Ok(())},
            "test"=>{services.host.alert(nubi_core::state::Alert{ts:nubi_core::state::now_ms(),provider:"claude".into(),kind:"done".into(),title:"Task complete".into(),body:"test notification".into(),session_key:None,queued:false});Ok(())},
            "hooks"=>{
                let enabled=with(&services.host,|s|!*s.hooks.get(key.as_str()).unwrap_or(&false));
                nubi_core::hooks_config::set_enabled(&key,enabled).map(|()|{with(&services.host,|s|{s.hooks.insert(key.to_string(),enabled);});services.host.emit_state();})
            },
            "desktime-tab"=>{
                let mut settings=with(&services.host,|s|s.settings.clone());settings.desktime_tab = !settings.desktime_tab;
                services.host.save_settings(settings)
            },
            "jump"|"new"|"compact"|"cancel-compact"|"close"=>{
                let host=services.host.clone();let key=key.to_string();let action=action.to_string();let weak=weak.clone();
                std::thread::spawn(move||{let result=crate::notch::actions::session_action(&host,&key,&action);if let Err(error)=&result {with(&host,|s|{if let Some(session)=s.sessions.get_mut(&key){session.action_error=Some(error.clone());}});host.emit_state();}let _=weak.upgrade_in_event_loop(move|panel|panel.global::<PanelData>().set_action_status(result.err().unwrap_or_default().into()));});Ok(())
            },
            "check-update"=>{
                if data.get_busy(){return;}data.set_busy(true);
                let host=services.host.clone();let weak=weak.clone();
                services.runtime.spawn(async move{let result=crate::updater::check_update(host).await;let _=weak.upgrade_in_event_loop(move|panel|{panel.global::<PanelData>().set_busy(false);panel.set_status(result.unwrap_or_else(|e|e).into());});});Ok(())
            },
            "release-notes"=>nubi_core::report::open_release_link(format!("https://github.com/hitraj2003/nubi/releases/tag/v{}",env!("CARGO_PKG_VERSION"))),
            "release-link"=>nubi_core::report::open_release_link(key.to_string()),
            "releases"=>{
                if data.get_release_loading(){return;}data.set_release_loading(true);
                data.set_release_status("Loading…".into());let host=services.host.clone();let weak=weak.clone();
                services.runtime.spawn(async move{let result=nubi_core::report::list_releases(host).await;let _=weak.upgrade_in_event_loop(move|panel|{let data=panel.global::<PanelData>();data.set_release_loading(false);match result{Ok(value)=>{let rows=release_rows(&value);data.set_release_status(if rows.is_empty(){"No releases yet"}else{""}.into());data.set_releases(model(rows));},Err(error)=>data.set_release_status(format!("Could not load versions. {error}").into())}});});Ok(())
            },
            "report"=>{
                data.set_report_open(true);data.set_report_status("".into());
                let host=services.host.clone();let weak=weak.clone();
                // Only immutable facts cross the worker boundary; editable fields stay on the UI thread.
                services.runtime.spawn(async move{let facts=nubi_core::report::report_context(host).await;let _=weak.upgrade_in_event_loop(move|panel|{panel.global::<PanelData>().invoke_action("report-context".into(),facts.to_string().into());});});Ok(())
            },
            "report-context"=>{
                if let Ok(Value::Object(facts))=serde_json::from_str(&key){let mut state=owner.borrow_mut();for(k,v)in facts{state.report.entry(k).or_insert(v);}state.sync_report();}Ok(())
            },
            "manual-report"=>{let mut state=owner.borrow_mut();state.report.entry("what-happened").or_insert_with(||json!(data.get_report_text().as_str()));state.sync_report();data.set_report_fields(true);Ok(())},
            "draft-report"=>{
                if data.get_report_busy(){return;}let text=data.get_report_text().trim().to_owned();if text.is_empty(){data.set_report_status("Write a sentence first".into());return;}
                data.set_report_busy(true);data.set_report_status(format!("{} is drafting…",if key=="claude"{"Claude"}else{"Codex"}).into());
                let context=json!(owner.borrow().report);let agent=key.to_string();let weak=weak.clone();
                services.runtime.spawn(async move{let result=nubi_core::report::draft_report(agent,text,context).await;let _=weak.upgrade_in_event_loop(move|panel|{let data=panel.global::<PanelData>();data.set_report_busy(false);match result{Ok(fields)=>data.invoke_action("report-draft".into(),fields.to_string().into()),Err(error)=>data.set_report_status(error.into())}});});Ok(())
            },
            "report-draft"=>{if let Ok(Value::Object(fields))=serde_json::from_str(&key){let mut state=owner.borrow_mut();state.report.extend(fields);state.sync_report();data.set_report_fields(true);data.set_report_status("Check the draft, then open it on GitHub.".into());}Ok(())},
            "open-report"=>{nubi_core::report::open_report(owner.borrow().report.clone());data.set_report_open(false);Ok(())},
            "extension-download"=>open("https://github.com/hitraj2003/nubi/releases/latest/download/nubi-desktime-extension.zip"),
            "extension-folder"=>{
                let folder=std::env::current_exe().ok().and_then(|p|p.parent().map(|p|p.join("desktime-extension"))).filter(|p|p.is_dir()).or_else(||{let p=std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../extension");p.is_dir().then_some(p)});
                folder.ok_or("Extension folder not found. Download and unzip the extension.".into()).and_then(|path|open(&path.to_string_lossy()))
            },
            _=>Err("Unknown panel action".into()),
        };
        if let Err(error)=result{panel.set_status(error.clone().into());data.set_action_status(error.into());}
        owner.borrow_mut().tick(&panel,&services.host,&services.notch);
    });
}
pub(crate) fn session_path(host: &NativeHost, key: &str) -> Option<String> {
    with(host, |state| {
        state
            .sessions
            .get(key)
            .and_then(|session| session.cwd.clone())
    })
}
fn open(target: &str) -> Result<(), String> {
    std::process::Command::new("explorer")
        .arg(target)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}
pub fn release_rows(value: &Value) -> Vec<PanelRelease> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .map(|r| PanelRelease {
            title: format!(
                "Version {}{}{}",
                r["version"].as_str().unwrap_or(""),
                if r["current"] == true {
                    " (installed)"
                } else {
                    ""
                },
                if r["prerelease"] == true {
                    " · pre-release"
                } else {
                    ""
                }
            )
            .into(),
            date: text(&r["date"]).into(),
            notes: text(&r["notes"]).into(),
            download: text(&r["download"]).into(),
            current: r["current"] == true,
        })
        .collect()
}
pub fn report_rows(values: &Map<String, Value>) -> Vec<PanelField> {
    [
        ("what-happened", "What happened", "textarea", &[][..]),
        ("expected", "What you expected", "textarea", &[][..]),
        ("steps", "Steps to reproduce", "textarea", &[][..]),
        (
            "area",
            "Part of Nubi",
            "select",
            &[
                "Notch",
                "Toast (notification pill and request cards)",
                "Settings panel",
                "Approvals (Allow / Deny)",
                "Claude questions",
                "Usage numbers",
                "Sounds",
                "Updates or installer",
                "Hooks or status line in Claude Code / Codex",
                "Other",
            ][..],
        ),
        (
            "provider",
            "Which agent",
            "select",
            &["Claude Code", "Codex", "Both", "Not related to an agent"][..],
        ),
        (
            "frequency",
            "How often",
            "select",
            &["Every time", "Sometimes", "Once"][..],
        ),
        ("setup", "Setup (filled in by Nubi)", "textarea", &[][..]),
    ]
    .into_iter()
    .map(|(key, label, kind, options)| {
        let value = values
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_else(|| options.first().copied().unwrap_or(""));
        PanelField {
            key: format!("report:{key}").into(),
            label: label.into(),
            kind: kind.into(),
            value: value.into(),
            options: model(options.iter().map(|s| (*s).into()).collect()),
            selected: options.iter().position(|s| *s == value).unwrap_or(0) as i32,
            enabled: true,
            ..Default::default()
        }
    })
    .collect()
}
