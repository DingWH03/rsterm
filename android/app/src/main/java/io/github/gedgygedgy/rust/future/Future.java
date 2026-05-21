package io.github.gedgygedgy.rust.future;

import io.github.gedgygedgy.rust.task.PollResult;
import io.github.gedgygedgy.rust.task.Waker;

/** Interface for asynchronous Java results polled from Rust. */
public interface Future<T> {
    PollResult<T> poll(Waker waker);
}
