package io.github.gedgygedgy.rust.ops;

final class FnBiFunctionImpl<T, U, R> implements FnBiFunction<T, U, R> {
    private final FnAdapter<FnBiFunctionImpl<T, U, R>, T, U, R> adapter;

    private FnBiFunctionImpl(FnAdapter<FnBiFunctionImpl<T, U, R>, T, U, R> adapter) {
        this.adapter = adapter;
    }

    @Override
    public R apply(T arg1, U arg2) {
        return this.adapter.call(this, arg1, arg2);
    }

    @Override
    public void close() {
        this.adapter.close();
    }
}
