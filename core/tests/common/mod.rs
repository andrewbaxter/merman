use merman3_core::context::{
    Context,
    ContextConfig,
};
use merman3_core::document::Document;
use merman3_core::matcher::match_document;
use merman3_core::measure::MeasureFixed;
use merman3_core::render::Snapshot;
use merman3_core::spec::SpecSyntax;
use merman3_core::syntax::Syntax;
use std::rc::Rc;

pub fn load_syntax(json: &str) -> Rc<Syntax> {
    let spec: SpecSyntax = serde_json::from_str(json).expect("syntax json");
    return Rc::new(Syntax::syntax_resolve(spec).unwrap_or_else(|e| panic!("{}", e.join("\n"))));
}

pub fn load_document(syntax: &Syntax, text: &str) -> Rc<Document> {
    let value: serde_json::Value = serde_json::from_str(text).unwrap();
    return Rc::new(match_document(syntax, &value).unwrap_or_else(|e| panic!("{}", e.mismatch_format())));
}

pub struct Clock(pub f64);

impl Clock {
    pub fn now(&mut self) -> f64 {
        self.0 += 1.;
        return self.0;
    }
}

pub fn build(syntax: Rc<Syntax>, document: Rc<Document>, converse: f64, transverse: f64) -> Context {
    return Context::context_new(
        syntax,
        document,
        ContextConfig::default(),
        Box::new(MeasureFixed),
        converse,
        transverse,
    );
}

pub fn settle(ctx: &mut Context, clock: &mut Clock) {
    let mut guard = 0;
    loop {
        let requested = ctx.take_timer_request();
        if !requested && ctx.iteration_idle() {
            break;
        }
        ctx.handle_timer(&mut || clock.now());
        guard += 1;
        assert!(guard < 100_000, "layout didn't settle");
    }
}

pub fn render_text(snapshot: &Snapshot, unit: f64) -> Vec<String> {
    let mut out = vec![];
    for row in &snapshot.rows {
        let mut line = String::new();
        for b in &row.bricks {
            let col = ((b.converse + b.pad_before) / unit).round() as usize;
            while line.chars().count() < col {
                line.push(' ');
            }
            line.push_str(&b.text);
        }
        out.push(line);
    }
    return out;
}
