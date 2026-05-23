package dev.rsTerminal.app;

import android.app.NativeActivity;
import android.os.Build;
import android.view.View;
import android.view.ViewGroup;
import android.view.inputmethod.InputMethodManager;
import android.window.OnBackInvokedCallback;
import android.window.OnBackInvokedDispatcher;

public class RsTerminalActivity extends NativeActivity {
    private Object backCallback = null;

    @Override
    protected void onStart() {
        super.onStart();
        registerBackCallback();
    }

    @Override
    protected void onStop() {
        unregisterBackCallback();
        super.onStop();
    }

    @Override
    public void onBackPressed() {
        // Fallback for older API levels (pre-33).
        nativeOnBackPressed();
    }

    /**
     * Called from Rust after committed text input on Android.
     *
     * Gboard and some other Android IMEs keep an internal composing/suggestion
     * buffer for non-EditText views. NativeActivity's content view is not a
     * real text editor, so the first backspace presses may be consumed by the
     * IME to clear that internal buffer instead of being delivered to winit/egui.
     * Restarting the input connection after committed text clears the stale
     * composing buffer while keeping the keyboard open.
     */
    public void restartRsTerminalInput() {
        runOnUiThread(new Runnable() {
            @Override
            public void run() {
                View target = findImeTargetView();
                if (target == null) {
                    return;
                }

                InputMethodManager imm = (InputMethodManager) getSystemService(INPUT_METHOD_SERVICE);
                if (imm != null) {
                    imm.restartInput(target);
                }
            }
        });
    }

    private View findImeTargetView() {
        View focused = getCurrentFocus();
        if (focused != null) {
            return focused;
        }

        View content = findViewById(android.R.id.content);
        if (content instanceof ViewGroup) {
            ViewGroup group = (ViewGroup) content;
            if (group.getChildCount() > 0) {
                return group.getChildAt(0);
            }
        }

        return getWindow() != null ? getWindow().getDecorView() : null;
    }

    private void registerBackCallback() {
        if (Build.VERSION.SDK_INT < 33 || backCallback != null) {
            return;
        }
        OnBackInvokedCallback callback = () -> nativeOnBackPressed();
        backCallback = callback;
        getOnBackInvokedDispatcher().registerOnBackInvokedCallback(
            OnBackInvokedDispatcher.PRIORITY_DEFAULT,
            callback
        );
    }

    private void unregisterBackCallback() {
        if (Build.VERSION.SDK_INT < 33 || backCallback == null) {
            return;
        }
        getOnBackInvokedDispatcher().unregisterOnBackInvokedCallback((OnBackInvokedCallback) backCallback);
        backCallback = null;
    }

    private static native void nativeOnBackPressed();
}
