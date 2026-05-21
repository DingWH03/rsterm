package io.github.gedgygedgy.rust.ops;

import java.io.Closeable;

public interface FnRunnable extends Runnable, Closeable {
    @Override
    void run();

    @Override
    void close();
}
