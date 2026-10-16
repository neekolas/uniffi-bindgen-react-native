{%- let name = callback.name()|ffi_callback_name %}
{%- let return_type = callback.return_type() %}
{%- let first_arg = callback.arguments()[0].name()|var_name|fmt("rs_{}") %}
namespace {{ ns }} {
    using namespace facebook;
    static const char callbackSlot = 0;
    static void body(jsi::Runtime &rt,
                     std::shared_ptr<uniffi_runtime::UniffiCallInvoker> callInvoker,
                     const jsi::Value &callbackValue, bool *argumentsHandedOff
            {%- if let Some(completion) = callback.async_completion(ci) %}
            , std::shared_ptr<uniffi_runtime::ForeignCompletion<{{ completion.name()|ffi_callback_name }}, {{ completion.completion_result().borrow()|ffi_type_name }}>> completionState
            {%- endif %}
            {%- for arg in callback.arguments() %}
            {%-   let arg_t = arg.type_().borrow()|ffi_type_name %}
            {%-   let arg_nm_rs = arg.name()|var_name|fmt("rs_{}") %}
            , {{- arg_t }} {{ arg_nm_rs }}
            {%- endfor %}
            {%- if callback.has_rust_call_status_arg() -%}
            , RustCallStatus* uniffi_call_status
            {%- endif -%}
            {%- match return_type -%}
            {%- when Some(return_type) %}
            , {{ return_type|ffi_type_name }}* uniffi_direct_return
            {%- when None -%}
            {%- endmatch -%}) {

        // Convert the arguments from Rust, into jsi::Values.
        // We'll use the Bridging class to do this…
        {%- for arg in callback.arguments_no_return() %}
        {%-   let arg_nm_rs = arg.name()|var_name|fmt("rs_{}") %}
        {%-   let arg_t = arg.type_().borrow()|ffi_type_name_from_js %}
        {%-   let arg_nm = arg.name()|var_name|fmt("js_{}") %}
        {%- if arg.name() == "uniffi_future_callback" %}
        auto {{ arg_nm }} = {{ arg.type_().borrow()|bridging_class(ci) }}::toJsOwned(rt, callInvoker, completionState);
        {%- else %}
        auto {{ arg_nm }} = {{ arg.type_().borrow()|bridging_class(ci) }}::toJs(rt, callInvoker, {{ arg_nm_rs }});
        {%- endif %}
        {%- endfor %}

        // Now we are ready to call the callback.
        // We are already on the JS thread, because this `body` function was
        // invoked from the CallInvoker.
        try {
            // Getting the callback function
            auto cb = callbackValue.asObject(rt).asFunction(rt);
            if (argumentsHandedOff) *argumentsHandedOff = true;
            auto uniffiResult = cb.call(rt
            {%- for arg in callback.arguments_no_return() %}
            {%-   let arg_nm = arg.name()|var_name|fmt("js_{}") -%}
                , {{ arg_nm }}
            {%- endfor %}
            );

            {% if callback.has_rust_call_status_arg() -%}
            // Now copy the result back from JS into the RustCallStatus object.
            {{ ci.cpp_namespace() }}::Bridging<RustCallStatus>::copyFromJs(rt, callInvoker, uniffiResult, uniffi_call_status);

            if (uniffi_call_status->code != UNIFFI_CALL_STATUS_OK) {
                // The JS callback finished abnormally, so we cannot retrieve the return value.
                return;
            }
            {%- endif %}

            {% match callback.arg_return_type() -%}
            {%- when Some with (arg_t) %}
            {%- let return_var = callback.arg_return_cpp_name() %}
            // return type is {{ arg_t|fmt("{:?}") }}
            {%- let is_async = arg_t.is_foreign_future() %}
            {%- let arg_t_label = arg_t|ffi_type_name_from_js %}
            // Finally, we need to copy the return value back into the Rust pointer.
            *{{ return_var }} =
                {{ arg_t|bridging_namespace(ci) }}::Bridging<
                {%- if is_async %}
                    {{ arg_t_label }}
                {%- else %}
                    ReferenceHolder<{{ arg_t_label }}>
                {%- endif %}
                >::fromJs(
                    rt, callInvoker, uniffiResult
                );
            {%- if is_async %}
            uniffi_runtime::HandleRegistry::instance().attach({{ return_var }}->handle, completionState);
            {%- endif %}
            {%- else %}
            {%- endmatch %}

            {%- match return_type -%}
            {%- when Some(return_type) %}
            // Write the direct return value back to the caller.
            if (uniffi_direct_return != nullptr) {
                *uniffi_direct_return = {{ return_type|bridging_class(ci) }}::fromJs(
                    rt, callInvoker, uniffiResult
                );
            }
            {%- when None -%}
            {%- endmatch %}
        } catch (const jsi::JSError &error) {
            std::cout << "Error in callback {{ name }}: "
                    << error.what() << std::endl;
            throw error;
        }
    }


    static {% match return_type %}{%- when Some(return_type) %}{{ return_type|ffi_type_name }}{%- when None %}void{%- endmatch %} callback(
            {%- for arg in callback.arguments() %}
            {{ arg.type_().borrow()|ffi_type_name }} {{ arg.name()|var_name|fmt("rs_{}") }}{% if !loop.last %}, {% endif %}
            {%- endfor %}
            {%- if callback.has_rust_call_status_arg() %}, RustCallStatus* uniffi_call_status{% endif %}) noexcept {
        {%- if return_type.is_some() %}
        {{ return_type.unwrap()|ffi_type_name }} uniffi_result{};
        {%- endif %}
        {%- if let Some(arg_t) = callback.arg_return_type() %}
        *{{ callback.arg_return_cpp_name() }} = {};
        {%- if arg_t.is_foreign_future() %}
        {{ callback.arg_return_cpp_name() }}->free = [](uint64_t) {};
        {%- endif %}
        {%- endif %}
        {%- if callback.has_rust_call_status_arg() %}
        *uniffi_call_status = {};
        {%- endif %}
        bool entered = false;
        bool dispatched = false;
        auto &handles = uniffi_runtime::HandleRegistry::instance();
        {%- if callback.is_drop_callback() %}
        handles.cancel({{ first_arg }});
        {%- endif %}
        {%- if let Some(completion) = callback.async_completion(ci) %}
        using Completion = uniffi_runtime::ForeignCompletion<{{ completion.name()|ffi_callback_name }}, {{ completion.completion_result().borrow()|ffi_type_name }}>;
        std::shared_ptr<Completion> completionState;
        {%- endif %}
        auto lease = handles.acquire({{ first_arg }}, "{{ callback.handle_kind() }}");
        {%- if !callback.is_blocking() %}
        // UniFFI invokes continuations under its scheduler lock. They must not
        // wait for JS, which may immediately poll that same future again.
        try {
            auto generation = lease.owner;
            auto retirement = std::make_shared<uniffi_runtime::HandleRetirement>({{ first_arg }}, std::move(lease));
            if (generation) {
                generation->invoker->invokeNonBlocking(generation->runtime,
                    [generation, retirement
                    {%- for arg in callback.arguments() %}, {{ arg.name()|var_name|fmt("rs_{}") }}{% endfor %}](jsi::Runtime &rt) {
                    retirement->run([&] {
                        auto found = generation->callbacks.find(&callbackSlot);
                        if (found == generation->callbacks.end()) return;
                        body(rt, generation->invoker, found->second, nullptr
                        {%- for arg in callback.arguments() %}, {{ arg.name()|var_name|fmt("rs_{}") }}{% endfor %});
                    });
                });
            }
        } catch (...) { /* C ABI callbacks cannot throw. */ }
        return;
        {%- else %}
        try {
            {%- if let Some(completion) = callback.async_completion(ci) %}
            completionState = std::make_shared<Completion>(rs_uniffiFutureCallback, rs_uniffiCallbackData, [] {
                {{ completion.completion_result().borrow()|ffi_type_name }} result{};
                result.call_status = {{ ci.cpp_namespace() }}::callbackFailure();
                return result;
            }, lease.owner);
            {%- endif %}
            if (lease) {
                auto generation = lease.owner;
                {%- if callback.async_completion(ci).is_some() %}
                { std::lock_guard<std::mutex> lock(generation->pendingMutex);
                  generation->pending.emplace(completionState.get(), completionState); }
                {%- endif %}
                dispatched = generation->invoker->invokeBlocking(generation->runtime, [&](jsi::Runtime &rt) {
                    auto found = generation->callbacks.find(&callbackSlot);
                    if (found == generation->callbacks.end()) return;
                    body(rt, generation->invoker, found->second, &entered
                        {%- if callback.async_completion(ci).is_some() %}, completionState{% endif %}
                        {%- for arg in callback.arguments() %}, {{ arg.name()|var_name|fmt("rs_{}") }}{% endfor %}
                        {%- if callback.has_rust_call_status_arg() %}, uniffi_call_status{% endif %}
                        {%- if return_type.is_some() %}, &uniffi_result{% endif %});
                }) && entered;
            }
        } catch (...) { dispatched = false; }
        if (!dispatched) {
            {%- if callback.is_continuation_callback() %}
            handles.release({{ first_arg }});
            {%- endif %}
            {%- if callback.is_clone_callback() %}
            uniffi_result = handles.retain({{ first_arg }});
            {%- endif %}
            {%- if callback.is_drop_callback() %}
            handles.cancel({{ first_arg }});
            handles.release({{ first_arg }});
            {%- endif %}
            {%- if callback.async_completion(ci).is_some() %}
            if (completionState) completionState->cancel();
            {%- endif %}
            {%- if callback.has_rust_call_status_arg() %}
            *uniffi_call_status = {{ ci.cpp_namespace() }}::callbackFailure();
            {%- endif %}
            if (!entered) {
                {%- for arg in callback.arguments_no_return() %}
                {%- if arg.type_().is_rust_buffer() %}
                {{ ci.cpp_namespace() }}::Bridging<RustBuffer>::rustbuffer_free({{ arg.name()|var_name|fmt("rs_{}") }});
                {%- endif %}
                {%- endfor %}
            }
        }
        {%- if return_type.is_some() %}return uniffi_result;{% endif %}
        {%- endif %}
    }
    [[maybe_unused]] static {{ name }} makeCallbackFunction(jsi::Runtime &rt,
        std::shared_ptr<uniffi_runtime::UniffiCallInvoker>, const jsi::Value &value) {
        auto generation = uniffi_runtime::callbackGeneration(rt);
        generation->callbacks.insert_or_assign(&callbackSlot, jsi::Value(rt, value));
        return callback;
    }
} // namespace {{ ns }}
