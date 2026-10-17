// Calling into Rust.
extern "C" {
    {%- for definition in ci.ffi_definitions() %}
    {%- match definition %}
    {%- when FfiDefinition::Struct(ffi_struct) %}
    {%- call cpp::callback_struct_decl(ffi_struct) %}
    {%- when FfiDefinition::CallbackFunction(callback) %}
    {%- call cpp::callback_fn_decl(callback) %}
    {%- when FfiDefinition::Function with(func) %}
    {% call cpp::rust_fn_decl(func) %}
    {%- else %}
    {%- endmatch %}
    {%- endfor %}
}
