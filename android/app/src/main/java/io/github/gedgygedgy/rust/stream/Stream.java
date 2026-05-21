package io.github.gedgygedgy.rust.stream;

import io.github.gedgygedgy.rust.task.PollResult;
import io.github.gedgygedgy.rust.task.Waker;

/** Interface for asynchronous Java streams polled from Rust. */
public interface Stream<T> {
    PollResult<StreamPoll<T>> pollNext(Waker waker);
}
