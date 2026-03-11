use core_x::dyld::Library;
use core_x::ffi::{NativeType, Signature, Value};
use core_x::foreign::ForeignFunction;
use std::ffi::CString;
use std::sync::Arc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let lib: Arc<Library> =
        Arc::from(Library::open("/usr/lib/libSystem.B.dylib")?);
    let puts = ForeignFunction::new(
        lib.clone(),
        "puts",
        Signature::new(vec![NativeType::Ptr], NativeType::I32),
    )?;

    let strlen = ForeignFunction::new(
        lib,
        "strlen",
        Signature::new(vec![NativeType::Ptr], NativeType::USize),
    )?;
    let msg = CString::new("hello from dynamic call layer")
        .expect("string literal contains no interior NUL");

    let puts_result = puts.call(&[Value::from_c_string(&msg)])?;
    let len_result = strlen.call(&[Value::from_c_string(&msg)])?;

    match (puts_result, len_result) {
        (Value::I32(rc), Value::USize(len)) => {
            println!("puts returned {rc}, len returned: {len}");
        }
        other => eprintln!("unexpected return type: {other:?}"),
    }

    Ok(())
}
