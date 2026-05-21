package io.github.gedgygedgy.rust.panic;

public class PanicException extends RuntimeException {
    public PanicException(String message) {
        super(message);
    }
}
