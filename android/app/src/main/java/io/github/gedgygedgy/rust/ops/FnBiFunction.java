package io.github.gedgygedgy.rust.ops;

import java.io.Closeable;

public interface FnBiFunction<T, U, R> extends Closeable {
    R apply(T arg1, U arg2);

    @Override
    void close();
}
