{%- let cb_name = callback.name()|ffi_callback_name %}
namespace {{ ci.cpp_namespace() }} {
using CallInvoker = uniffi_runtime::UniffiCallInvoker;

template <> struct Bridging<{{ cb_name }}> {
  using Result = {{ callback.completion_result().borrow()|ffi_type_name }};
  using Completion = uniffi_runtime::ForeignCompletion<{{ cb_name }}, Result>;
  static void discardOwnedBuffer(jsi::Runtime &rt, std::shared_ptr<CallInvoker> callInvoker,
      const jsi::Value &value) {
    if (!value.isObject()) return;
    auto view = value.asObject(rt);
    if (!view.hasProperty(rt, uniffi_jsi::kUbrnRustCapacity) ||
        view.getProperty(rt, uniffi_jsi::kUbrnRustCapacity).asNumber() == 0) return;
    auto buffer = Bridging<RustBuffer>::fromJs(rt, callInvoker, value);
    Bridging<RustBuffer>::rustbuffer_free(buffer);
  }
  static jsi::Value toJsOwned(jsi::Runtime &rt, std::shared_ptr<CallInvoker> callInvoker,
      std::shared_ptr<Completion> completion) {
    return jsi::Function::createFromHostFunction(rt,
        jsi::PropNameID::forAscii(rt, "completeForeignFuture"), 2,
        [completion, callInvoker](jsi::Runtime &rt, const jsi::Value &, const jsi::Value *args, size_t count) {
          if (count != 2) throw jsi::JSError(rt, "Invalid foreign future completion");
          // JS lowering may already have allocated non-owning Rust-backed
          // views. Retired completions must consume those allocations too.
          // A zero capacity stamp identifies an already-consumed view.
          if (completion->consumed.load()) {
            if (args[1].isObject()) {
              auto result = args[1].asObject(rt);
              {%- if callback.completion_returns_buffer(ci) %}
              discardOwnedBuffer(rt, callInvoker, result.getProperty(rt, "return_value"));
              {%- endif %}
              auto status = result.getProperty(rt, "call_status");
              if (status.isObject()) {
                discardOwnedBuffer(rt, callInvoker, status.asObject(rt).getProperty(rt, "errorBuf"));
              }
            }
            return jsi::Value::undefined();
          }
          auto result = Bridging<Result>::fromJs(rt, callInvoker, args[1]);
          if (!completion->complete(result)) {
            {%- if callback.completion_returns_buffer(ci) %}
            Bridging<RustBuffer>::rustbuffer_free(result.return_value);
            {%- endif %}
            Bridging<RustBuffer>::rustbuffer_free(result.call_status.error_buf);
          }
          return jsi::Value::undefined();
        });
  }
  static jsi::Value toJs(jsi::Runtime &rt, std::shared_ptr<CallInvoker> callInvoker, {{ cb_name}} rsCallback) {
    {%- let cb_id = callback.name()|fmt("--{}") %}
    return jsi::Function::createFromHostFunction(
        rt,
        jsi::PropNameID::forAscii(rt, "{{ cb_id }}"),
        {{ callback.arguments().len() }},
        [rsCallback, callInvoker](
            jsi::Runtime &rt,
            const jsi::Value &thisValue,
            const jsi::Value *arguments,
            size_t count) -> jsi::Value
        {
            return intoRust(rt, callInvoker, thisValue, arguments, count, rsCallback);
        }
    );
  }

  static jsi::Value intoRust(
      jsi::Runtime &rt,
      std::shared_ptr<CallInvoker> callInvoker,
      const jsi::Value &thisValue,
      const jsi::Value *args,
      size_t count,
      {{ cb_name}} func) {
    // Convert the arguments into the Rust, with Bridging<T>::fromJs,
    // then call the rs_callback with those arguments.
    {%- call cpp::cpp_fn_rust_caller_body_with_func_name(callback, "func") %}
  }
};
} // namespace {{ ci.cpp_namespace() }}
