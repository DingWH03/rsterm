package io.github.gedgygedgy.rust.task;

import io.github.gedgygedgy.rust.ops.FnRunnable;
import java.io.Closeable;

/** Wraps a std::task::Waker in a Java object. */
public final class Waker implements Closeable {
    private final FnRunnable wakeRunnable;

    private Waker(FnRunnable wakeRunnable) {
        this.wakeRunnable = wakeRunnable;
    }

    public void wake() {
        this.wakeRunnable.run();
    }

    @Override
    public void close() {
        this.wakeRunnable.close();
    }
}
