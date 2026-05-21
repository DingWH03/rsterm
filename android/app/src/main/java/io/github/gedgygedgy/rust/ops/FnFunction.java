package io.github.gedgygedgy.rust.ops;

import java.io.Closeable;

public interface FnFunction<T, R> extends Closeable {
    R apply(T arg);

    @Override
    void close();
}
