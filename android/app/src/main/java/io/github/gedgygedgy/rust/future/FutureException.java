package io.github.gedgygedgy.rust.future;

/** Exception wrapper used by Future implementations. */
public class FutureException extends RuntimeException {
    public FutureException(Throwable cause) {
        super(cause);
    }
}
