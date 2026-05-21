package io.github.gedgygedgy.rust.ops;

final class FnFunctionImpl<T, R> implements FnFunction<T, R> {
    private final FnAdapter<FnFunctionImpl<T, R>, T, Object, R> adapter;

    private FnFunctionImpl(FnAdapter<FnFunctionImpl<T, R>, T, Object, R> adapter) {
        this.adapter = adapter;
    }

    @Override
    public R apply(T arg) {
        return this.adapter.call(this, arg, null);
    }

    @Override
    public void close() {
        this.adapter.close();
    }
}
