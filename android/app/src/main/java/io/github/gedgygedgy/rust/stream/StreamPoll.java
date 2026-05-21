package io.github.gedgygedgy.rust.stream;

/** Represents the result of polling an async stream. */
public interface StreamPoll<T> {
    T get();
}
