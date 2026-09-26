use {
    merman_core::{
        context::{
            Context,
            ContextConfig,
        },
        display::DisplayTest,
        document::Document,
        environment::EnvironmentTest,
        matcher::match_document,
        spec::SpecSyntax,
        syntax::Syntax,
    },
    std::rc::Rc,
};

pub fn build(
    syntax: Rc<Syntax>,
    document: Rc<Document>,
    converse: f64,
    transverse: f64,
) -> (Context, DisplayTest, EnvironmentTest) {
    let display = DisplayTest::default();
    let environment = EnvironmentTest::default();
    let context = Context::context_new(syntax, document, ContextConfig {
        lay_beyond_view: f64::INFINITY,
        ..ContextConfig::default()
    }, Box::new(display.clone()), Box::new(environment.clone()), converse, transverse);
    return (context, display, environment);
}

pub fn load_document(syntax: &Syntax, text: &str) -> Rc<Document> {
    let value: serde_json::Value = serde_json::from_str(text).unwrap();
    return Rc::new(match_document(syntax, &value).unwrap_or_else(|e| panic!("{}", e.mismatch_format())));
}

pub fn load_syntax(json: &str) -> Rc<Syntax> {
    let spec: SpecSyntax = serde_json::from_str(json).expect("syntax json");
    return Rc::new(Syntax::syntax_resolve(spec).unwrap_or_else(|e| panic!("{}", e)));
}

pub fn render_text(display: &DisplayTest, unit: f64) -> Vec<String> {
    let mut out = vec![];
    for row in display.display_test_rows() {
        let mut line = String::new();
        for b in &row.bricks {
            let col = (b.converse / unit).round() as usize;
            while line.chars().count() < col {
                line.push(' ');
            }
            line.push_str(&b.text);
        }
        out.push(line);
    }
    return out;
}

pub fn settle(ctx: &mut Context) {
    let mut guard = 0;
    loop {
        let requested = ctx.take_timer_request();
        if !requested && ctx.iteration_idle() {
            break;
        }
        ctx.handle_timer();
        guard += 1;
        assert!(guard < 100_000, "layout didn't settle");
    }
}
