use clap::Parser as CParser;
use inkwell::types::BasicType;
use pest::Parser;

use std::io::Read;

#[derive(pest_derive::Parser)]
#[grammar = "bf.pest"]
struct BFGrammar;

#[derive(Debug)]
enum Node {
    Left,
    Right,
    Add,
    Sub,
    Read,
    Write,
    Loop(Vec<Node>),
}

fn build_ast_from_op(pair: pest::iterators::Pair<Rule>) -> Node {
    match pair.as_str() {
        "<" => Node::Left,
        ">" => Node::Right,
        "+" => Node::Add,
        "-" => Node::Sub,
        "," => Node::Read,
        "." => Node::Write,
        other => panic!("error parsing expression: {}", other),
    }
}

fn build_ast_from_loop(pair: pest::iterators::Pair<Rule>) -> Node {
    if let Rule::r#loop = pair.as_rule() {
        let pairs = pair.into_inner();
        let mut inner = Vec::new();
        for pair in pairs {
            match pair.as_rule() {
                Rule::op => inner.push(build_ast_from_op(pair)),
                Rule::r#loop => inner.push(build_ast_from_loop(pair)),
                other => panic!("error parsing loop: {:?}", other),
            }
        }

        Node::Loop(inner)
    } else {
        panic!("error parsing loop");
    }
}

#[allow(clippy::too_many_arguments)]
fn build_tree(
    ast: &[Node],
    builder: &inkwell::builder::Builder,
    context: &inkwell::context::Context,
    fn_value: inkwell::values::FunctionValue,
    pointer: inkwell::values::PointerValue,
    data: inkwell::values::PointerValue,
    print_fn: inkwell::values::FunctionValue,
    read_fn: inkwell::values::FunctionValue,
) {
    let i8_type = context.i8_type();
    let i32_type = context.i32_type();

    for node in ast {
        let ptr = builder
            .build_load(i32_type, pointer, "ptr")
            .unwrap()
            .into_int_value();
        let cur_ptr = unsafe { builder.build_gep(i8_type, data, &[ptr], "cur_ptr") }.unwrap();
        let cur_val = builder
            .build_load(i8_type, cur_ptr, "cur_val")
            .unwrap()
            .into_int_value();

        match node {
            Node::Left => {
                let dec = builder
                    .build_int_sub(ptr, i32_type.const_int(1, false), "dec")
                    .unwrap();
                builder.build_store(pointer, dec).unwrap();
            }
            Node::Right => {
                let inc = builder
                    .build_int_add(ptr, i32_type.const_int(1, false), "sub")
                    .unwrap();
                builder.build_store(pointer, inc).unwrap();
            }
            Node::Add => {
                let inc = builder
                    .build_int_add(cur_val, i8_type.const_int(1, false), "inc")
                    .unwrap();
                builder.build_store(cur_ptr, inc).unwrap();
            }
            Node::Sub => {
                let dec = builder
                    .build_int_sub(cur_val, i8_type.const_int(1, false), "dec")
                    .unwrap();
                builder.build_store(cur_ptr, dec).unwrap();
            }
            Node::Write => {
                builder
                    .build_call(print_fn, &[cur_val.into()], "write")
                    .unwrap();
            }
            Node::Read => {
                let byte = builder
                    .build_call(read_fn, &[], "read")
                    .unwrap()
                    .try_as_basic_value()
                    .unwrap_left()
                    .into_int_value();
                builder.build_store(cur_ptr, byte).unwrap();
            }
            Node::Loop(inner) => {
                let condition = builder
                    .build_int_compare(
                        inkwell::IntPredicate::NE,
                        cur_val,
                        i8_type.const_int(0, false),
                        "cmp",
                    )
                    .unwrap();

                let loop_start = context.append_basic_block(fn_value, "start");
                let loop_end = context.append_basic_block(fn_value, "end");

                builder
                    .build_conditional_branch(condition, loop_start, loop_end)
                    .unwrap();

                builder.position_at_end(loop_start);
                build_tree(
                    inner, builder, context, fn_value, pointer, data, print_fn, read_fn,
                );
                let real_ptr = builder.build_load(i32_type, pointer, "real_ptr").unwrap();
                let real_cur_ptr = unsafe {
                    builder.build_gep(i8_type, data, &[real_ptr.into_int_value()], "real_cur_ptr")
                }
                .unwrap();
                let real_cur_val = builder
                    .build_load(i8_type, real_cur_ptr, "real_cur_ptr")
                    .unwrap()
                    .into_int_value();
                let real_cmp = builder
                    .build_int_compare(
                        inkwell::IntPredicate::NE,
                        real_cur_val,
                        i8_type.const_int(0, false),
                        "real_cmp",
                    )
                    .unwrap();
                builder
                    .build_conditional_branch(real_cmp, loop_start, loop_end)
                    .unwrap();

                builder.position_at_end(loop_end);
            }
        }
    }
}

fn build(ast: &[Node]) {
    // llvm boilerplate
    let context = inkwell::context::Context::create();
    let module = context.create_module("fuckbrain");
    let builder = context.create_builder();
    let i8_type = context.i8_type();

    // link to external print function
    let print_fn_type = context.void_type().fn_type(&[i8_type.into()], false);
    let print_fn = module.add_function("print_byte", print_fn_type, None);

    // link to external read function
    let read_fn_type = i8_type.fn_type(&[], false);
    let read_fn = module.add_function("read_byte", read_fn_type, None);

    // create main function
    let fn_type = context.void_type().fn_type(&[], false);
    let fn_value = module.add_function("main", fn_type, None);
    let basic_block = context.append_basic_block(fn_value, "entry");

    // start builder within main function
    builder.position_at_end(basic_block);

    // create data array
    let data = builder
        .build_alloca(i8_type.array_type(30000), "data")
        .unwrap();
    builder
        .build_store(data, i8_type.array_type(30000).const_zero())
        .unwrap();
    let i32_type = context.i32_type();
    let pointer = builder.build_alloca(i32_type, "pointer").unwrap();
    builder
        .build_store(pointer, i32_type.const_int(0, false))
        .unwrap();

    // actual code generation from brainfuck
    build_tree(
        ast, &builder, &context, fn_value, pointer, data, print_fn, read_fn,
    );

    // return void from main
    builder.build_return(None).unwrap();

    // print ir
    println!("{}", module.to_string());
}

#[derive(clap::Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    file: Box<std::path::Path>,
}

const VALID_TOKENS: [char; 8] = ['>', '<', '+', '-', ',', '.', '[', ']'];

fn main() {
    let args = Args::parse();
    let mut input = String::new();
    std::fs::File::open(args.file)
        .unwrap()
        .read_to_string(&mut input)
        .unwrap();
    input.retain(|s| VALID_TOKENS.contains(&s));

    let pairs = BFGrammar::parse(Rule::program, &input).unwrap();

    let mut ast = Vec::new();

    for pair in pairs {
        match pair.as_rule() {
            Rule::op => ast.push(build_ast_from_op(pair)),
            Rule::r#loop => ast.push(build_ast_from_loop(pair)),
            _ => unreachable!(),
        }
    }

    build(&ast);
}
