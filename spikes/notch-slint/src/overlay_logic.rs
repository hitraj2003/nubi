//! In-memory request lifecycle. No production backend or settings access.
use crate::{OverlayFixture, OverlayQuestion, ParityOverlay};
use slint::{ComponentHandle, Model, ModelRc, VecModel};
use std::{cell::RefCell, rc::Rc, time::Instant};

struct State {
    questions: Vec<OverlayQuestion>,
    selections: Vec<Vec<String>>,
    left: f64,
    total: f64,
    last: Instant,
    held: bool,
}

fn show_question(ui: &ParityOverlay, state: &State) {
    let index = ui.get_step() as usize;
    let mut question = state.questions[index].clone();
    question.options = ModelRc::new(VecModel::from(
        question
            .options
            .iter()
            .map(|mut option| {
                option.selected = state.selections[index].contains(&option.label.to_string());
                option
            })
            .collect::<Vec<_>>(),
    ));
    ui.set_current_question(question);
}

fn advance(ui: &ParityOverlay, state: &mut State) {
    if !ui.get_outcome().is_empty() || state.selections[ui.get_step() as usize].is_empty() {
        return;
    }
    if ui.get_step() as usize + 1 < state.questions.len() {
        ui.set_step(ui.get_step() + 1);
        ui.set_custom("".into());
        ui.set_focus_index(-1);
        show_question(ui, state);
        ui.set_focus_index(0);
    } else {
        ui.set_answers(ModelRc::new(VecModel::from(
            state
                .selections
                .iter()
                .map(|labels| labels.join(", ").into())
                .collect::<Vec<_>>(),
        )));
        ui.set_outcome("answered".into());
    }
}

pub fn clock(seconds: f64) -> String {
    let seconds = seconds.ceil().max(0.) as u64;
    if seconds < 60 {
        format!("{seconds}s")
    } else {
        format!("{}:{:02}", seconds / 60, seconds % 60)
    }
}

pub fn connect(ui: &ParityOverlay, fixture: &str) {
    let data = ui.global::<OverlayFixture>();
    ui.set_request(match fixture {
        "parameters" => data.get_parameters(),
        "question" => data.get_question(),
        _ => data.get_approval(),
    });
    ui.set_question(fixture == "question");
    let questions: Vec<_> = data.get_questions().iter().collect();
    let seconds = f64::from(ui.get_request().seconds);
    let state = Rc::new(RefCell::new(State {
        selections: vec![Vec::new(); questions.len()],
        questions,
        left: seconds,
        total: seconds,
        last: Instant::now(),
        held: ui.get_held(),
    }));
    ui.set_question_count(state.borrow().questions.len() as i32);
    show_question(ui, &state.borrow());
    ui.set_countdown(format!("Terminal takes over in {}", clock(seconds)).into());
    let weak = ui.as_weak();
    let shared = state.clone();
    ui.on_choose(move |index| {
        let Some(ui) = weak.upgrade() else { return };
        if !ui.get_outcome().is_empty() {
            return;
        }
        let mut state = shared.borrow_mut();
        let step = ui.get_step() as usize;
        let Some(option) = state.questions[step].options.row_data(index as usize) else {
            return;
        };
        let label = option.label.to_string();
        if state.questions[step].multi {
            if state.selections[step].contains(&label) {
                state.selections[step].retain(|item| item != &label);
            } else {
                state.selections[step].push(label);
            }
            show_question(&ui, &state);
        } else {
            state.selections[step] = vec![label];
            advance(&ui, &mut state);
        }
    });
    let weak = ui.as_weak();
    let shared = state.clone();
    ui.on_typed(move |text| {
        let Some(ui) = weak.upgrade() else { return };
        if !ui.get_outcome().is_empty() {
            return;
        }
        let mut state = shared.borrow_mut();
        let step = ui.get_step() as usize;
        let question = state.questions[step].clone();
        state.selections[step].retain(|label| {
            question
                .options
                .iter()
                .any(|option| option.label == label.as_str())
        });
        if !text.trim().is_empty() {
            if !question.multi {
                state.selections[step].clear();
            }
            if !state.selections[step]
                .iter()
                .any(|label| label == text.trim())
            {
                state.selections[step].push(text.trim().to_owned());
            }
        }
        show_question(&ui, &state);
    });
    let weak = ui.as_weak();
    let shared = state.clone();
    ui.on_advance(move || {
        if let Some(ui) = weak.upgrade() {
            advance(&ui, &mut shared.borrow_mut());
        }
    });
    let weak = ui.as_weak();
    let shared = state.clone();
    ui.on_custom_advance(move || {
        if let Some(ui) = weak.upgrade()
            && !ui.get_custom().trim().is_empty()
        {
            advance(&ui, &mut shared.borrow_mut());
        }
    });
    let weak = ui.as_weak();
    ui.on_decide(move |decision| {
        if let Some(ui) = weak.upgrade()
            && ui.get_outcome().is_empty()
        {
            ui.set_outcome(decision);
        }
    });
    let weak = ui.as_weak();
    ui.on_jump(move || {
        // Production opens the terminal without resolving the request.
        if let Some(ui) = weak.upgrade() {
            ui.set_terminal_requested(true);
        }
    });
    let weak = ui.as_weak();
    ui.on_resolved(move || {
        if let Some(ui) = weak.upgrade() {
            let _ = ui.hide();
        }
    });
    let weak = ui.as_weak();
    ui.on_tick(move || {
        let Some(ui) = weak.upgrade() else { return };
        let mut state = state.borrow_mut();
        let now = Instant::now();
        // Pause locally on hover; external holds use the same clock. Frozen renders do not tick.
        if !state.held && !ui.get_held() {
            state.left = (state.left - now.duration_since(state.last).as_secs_f64()).max(0.);
        }
        state.last = now;
        state.held = ui.get_held();
        ui.set_countdown(format!("Terminal takes over in {}", clock(state.left)).into());
        ui.set_remaining((state.left / state.total.max(1.)) as f32);
        if state.left <= 0. {
            ui.set_outcome("takeover".into());
        }
    });
}
