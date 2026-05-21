package io.github.gedgygedgy.rust.stream;

import io.github.gedgygedgy.rust.task.PollResult;
import io.github.gedgygedgy.rust.task.Waker;
import java.util.LinkedList;
import java.util.Queue;

/** Simple queue-backed Stream implementation. */
public class QueueStream<T> implements Stream<T> {
    private Waker waker = null;
    private final Queue<T> result = new LinkedList<>();
    private boolean finished = false;
    private final Object lock = new Object();

    public QueueStream() {
    }

    @Override
    public PollResult<StreamPoll<T>> pollNext(Waker waker) {
        PollResult<StreamPoll<T>> result = null;
        Waker oldWaker = null;
        synchronized (this.lock) {
            if (!this.result.isEmpty()) {
                result = () -> () -> this.result.remove();
            } else if (this.finished) {
                result = () -> null;
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

    private void doEvent(Runnable runnable) {
        Waker waker = null;
        synchronized (this.lock) {
            assert !this.finished;
            runnable.run();
            waker = this.waker;
        }
        if (waker != null) {
            waker.wake();
        }
    }

    public void add(T item) {
        this.doEvent(() -> this.result.add(item));
    }

    public void finish() {
        this.doEvent(() -> this.finished = true);
    }
}
