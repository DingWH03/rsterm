package io.github.gedgygedgy.rust.ops;

final class FnRunnableImpl implements FnRunnable {
    private final FnAdapter<FnRunnableImpl, Object, Object, Object> adapter;

    private FnRunnableImpl(FnAdapter<FnRunnableImpl, Object, Object, Object> adapter) {
        this.adapter = adapter;
    }

    @Override
    public void run() {
        this.adapter.call(this, null, null);
    }

    @Override
    public void close() {
        this.adapter.close();
    }
}
