// Rust may retain a callback vtable across any number of JS runtime reloads.
// Each ABI/type therefore has one immutable, process-lifetime table containing
// static trampolines. Those trampolines route by generation-owned handles.
namespace {{ registry }} {
    static std::mutex vtableMutex;

    template <typename T>
    static T* putTable([[maybe_unused]] std::string_view identifier, T vtable) {
        static T table = vtable;
        return &table;
    }
} // namespace {{ registry }}
