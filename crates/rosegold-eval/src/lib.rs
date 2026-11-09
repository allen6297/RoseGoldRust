mod bytecode;
mod interp;
mod json;
mod value;

pub use bytecode::Fn as ByteFn;

pub use interp::{
    dump_ir, emit_llvm, exec_native_file, link_native, run_file, run_source, test_file,
    test_source, GuiElement, GuiScreen, Native, NativeFile, Outcome, Session, SettingOption,
};
pub use value::Value;
