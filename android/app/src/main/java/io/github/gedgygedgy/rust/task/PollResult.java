package io.github.gedgygedgy.rust.task;

/** Represents the result of polling an async future. */
public interface PollResult<T> {
    T get();
}
