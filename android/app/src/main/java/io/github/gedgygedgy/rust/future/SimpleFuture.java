package io.github.gedgygedgy.rust.future;

import io.github.gedgygedgy.rust.task.PollResult;
import io.github.gedgygedgy.rust.task.Waker;

/** Simple implementation of Future that can be completed exactly once. */
public class SimpleFuture<T> implements Future<T> {
    private Waker waker = null;
    private PollResult<T> result;
    private final Object lock = new Object();

    public SimpleFuture() {
    }

    @Override
    public PollResult<T> poll(Waker waker) {
        PollResult<T> result = null;
        Waker oldWaker = null;
        synchronized (this.lock) {
            if (this.result != null) {
                result = this.result;
            } else {
                oldWaker = this.waker;
                this.waker = waker;
            }
        }
        if (oldWaker != null) {
            oldWaker.close();
        }
        if (result != null) {
            waker.close();
        }
        return result;
    }

    private void wakeInternal(PollResult<T> result) {
        Waker waker = null;
        synchronized (this.lock) {
            assert this.result == null;
            this.result = result;
            waker = this.waker;
        }
        if (waker != null) {
            waker.wake();
        }
    }

    public void wake(T result) {
        this.wakeInternal(() -> result);
    }

    public void wakeWithThrowable(Throwable result) {
        this.wakeInternal(() -> {
            throw new FutureException(result);
        });
    }
}
