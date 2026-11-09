use rosegold_eval::{Session, Value};

#[test]
fn world_questions_go_to_the_host_callback() {
    let source = r#"
fn broke(id: String, x: Int, y: Int, z: Int, left: String): Bool {
    print(world.block(x, y, z) + " " + world.replaces(id) + " " + left);
    return world.light(id) == 12;
}

fn main(): Int {
    hooks.after_break("read", broke);
    return 0;
}
"#;
    let mut session = Session::load(source, "mod.rg", None).unwrap();
    assert_eq!(
        session.hooks(),
        vec![("after_break".to_string(), "read".to_string())]
    );
    let (value, printed) = session
        .call_hook_hosted(
            0,
            vec![
                Value::String("lantern".into()),
                Value::Int(1),
                Value::Int(2),
                Value::Int(3),
                Value::String("air".into()),
            ],
            &mut |name, _args| match name {
                "world.block" => Ok(Value::String("lantern".into())),
                "world.replaces" => Ok(Value::String("air".into())),
                "world.light" => Ok(Value::Int(12)),
                other => Err(format!("unexpected {other}")),
            },
        )
        .unwrap();
    assert!(matches!(value, Value::Bool(true)));
    assert!(printed.contains("lantern air air"));
}

#[test]
fn world_outside_a_hook_does_not_load() {
    let source = "fn main(): Int { print(world.block(0, 0, 0)); return 0; }\n";
    let Err(err) = Session::load(source, "mod.rg", None) else {
        panic!("main called world and still loaded");
    };
    assert!(
        err.contains("world.block is only available inside a hook"),
        "{err}"
    );
}
