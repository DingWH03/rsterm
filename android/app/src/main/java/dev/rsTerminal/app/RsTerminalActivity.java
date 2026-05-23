package dev.rsTerminal.app;

import android.app.NativeActivity;
import android.content.Context;
import android.os.Build;
import android.text.InputType;
import android.view.KeyCharacterMap;
import android.view.KeyEvent;
import android.view.View;
import android.view.ViewGroup;
import android.view.inputmethod.BaseInputConnection;
import android.view.inputmethod.EditorInfo;
import android.view.inputmethod.InputConnection;
import android.view.inputmethod.InputMethodManager;
import android.widget.FrameLayout;
import android.window.OnBackInvokedCallback;
import android.window.OnBackInvokedDispatcher;

/**
 * Android activity glue for rsTerminal.
 *
 * NativeActivity's default IME target can keep a hidden editable/composing
 * buffer. When that happens, after typing N characters the IME consumes the
 * first N Backspace taps by deleting its hidden buffer before the app sees a
 * Backspace.  The wrapper below supplies a small dummy InputConnection that
 * never keeps such a buffer. Text and delete commands are translated back into
 * normal KeyEvents for the original NativeActivity content view, so input still
 * flows through winit/egui's normal path and no JNI text callbacks are needed.
 */
public class RsTerminalActivity extends NativeActivity {
    private Object backCallback = null;
    private ImeBridgeLayout imeBridgeView = null;
    private View nativeContentView = null;

    @Override
    public void setContentView(View view) {
        if (shouldWrapNativeContentView(view)) {
            super.setContentView(wrapNativeContentView(view));
        } else {
            super.setContentView(view);
        }
    }

    @Override
    public void setContentView(View view, ViewGroup.LayoutParams params) {
        if (shouldWrapNativeContentView(view)) {
            super.setContentView(wrapNativeContentView(view), params);
        } else {
            super.setContentView(view, params);
        }
    }

    private boolean shouldWrapNativeContentView(View view) {
        return imeBridgeView == null
            && view != null
            && "android.app.NativeActivity$NativeContentView".equals(view.getClass().getName());
    }

    private View wrapNativeContentView(View nativeView) {
        nativeContentView = nativeView;

        ImeBridgeLayout bridge = new ImeBridgeLayout(this, nativeView);
        bridge.setLayoutParams(new ViewGroup.LayoutParams(
            ViewGroup.LayoutParams.MATCH_PARENT,
            ViewGroup.LayoutParams.MATCH_PARENT
        ));
        bridge.addView(nativeView, new FrameLayout.LayoutParams(
            ViewGroup.LayoutParams.MATCH_PARENT,
            ViewGroup.LayoutParams.MATCH_PARENT
        ));
        imeBridgeView = bridge;
        return bridge;
    }

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

    /** Called by Rust when the terminal surface is tapped. */
    public void showIme(final int mode) {
        runOnUiThread(new Runnable() {
            @Override
            public void run() {
                final View target = activateImeBridge();
                final InputMethodManager imm = (InputMethodManager) getSystemService(INPUT_METHOD_SERVICE);
                if (imm == null || target == null) {
                    return;
                }

                target.requestFocus();
                imm.restartInput(target);
                showSoftInputNowAndSoon(imm, target, mode);
            }
        });
    }

    private void showSoftInputNowAndSoon(
        final InputMethodManager imm,
        final View target,
        final int mode
    ) {
        // After a user hides the keyboard with Android Back, some IMEs ignore
        // the first SHOW_IMPLICIT request for an already-focused custom view.
        // This method is only called after an explicit terminal tap, so a forced
        // fallback is appropriate and keeps re-opening reliable.
        if (!imm.showSoftInput(target, mode)) {
            imm.showSoftInput(target, InputMethodManager.SHOW_FORCED);
        }

        target.postDelayed(new Runnable() {
            @Override
            public void run() {
                if (target.hasWindowFocus() && target.isFocused()) {
                    imm.showSoftInput(target, InputMethodManager.SHOW_FORCED);
                }
            }
        }, 80);
    }

    /** Called by NativeActivity/winit when egui hides the soft keyboard. */
    public void hideIme(final int mode) {
        runOnUiThread(new Runnable() {
            @Override
            public void run() {
                View target = imeBridgeView != null ? imeBridgeView : findImeTargetView();
                InputMethodManager imm = (InputMethodManager) getSystemService(INPUT_METHOD_SERVICE);
                if (imm != null && target != null) {
                    imm.hideSoftInputFromWindow(target.getWindowToken(), mode);
                    if (imeBridgeView != null) {
                        imeBridgeView.resetInputState();
                    }
                }
            }
        });
    }

    private View activateImeBridge() {
        if (imeBridgeView != null) {
            imeBridgeView.takeImeFocus();
            return imeBridgeView;
        }
        return findImeTargetView();
    }

    private View findImeTargetView() {
        if (nativeContentView != null) {
            return nativeContentView;
        }

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

    private static final class ImeBridgeLayout extends FrameLayout {
        private final View nativeTargetView;
        private RsTerminalInputConnection currentConnection;

        ImeBridgeLayout(Context context, View nativeTargetView) {
            super(context);
            this.nativeTargetView = nativeTargetView;
            setFocusable(true);
            setFocusableInTouchMode(true);
            setSaveEnabled(false);
        }

        void takeImeFocus() {
            if (!isFocused()) {
                requestFocus();
            }
        }

        void resetInputState() {
            if (currentConnection != null) {
                currentConnection.resetState();
            }
        }

        @Override
        public boolean onCheckIsTextEditor() {
            return true;
        }

        @Override
        public InputConnection onCreateInputConnection(EditorInfo outAttrs) {
            // Do not use TYPE_TEXT_VARIATION_VISIBLE_PASSWORD: several Android
            // keyboards switch to a password/safe layout for that flag.
            outAttrs.inputType = InputType.TYPE_CLASS_TEXT
                | InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS
                | InputType.TYPE_TEXT_FLAG_MULTI_LINE;
            outAttrs.imeOptions = EditorInfo.IME_ACTION_NONE
                | EditorInfo.IME_FLAG_NO_EXTRACT_UI
                | EditorInfo.IME_FLAG_NO_FULLSCREEN
                | EditorInfo.IME_FLAG_NO_PERSONALIZED_LEARNING;
            outAttrs.initialSelStart = 0;
            outAttrs.initialSelEnd = 0;

            currentConnection = new RsTerminalInputConnection(nativeTargetView);
            return currentConnection;
        }
    }

    private static final class RsTerminalInputConnection extends BaseInputConnection {
        private final View targetView;
        private String composingText = "";

        RsTerminalInputConnection(View targetView) {
            // dummyMode=true makes BaseInputConnection send committed text as
            // KeyEvents and clear its editable buffer instead of retaining text.
            super(targetView, true);
            this.targetView = targetView;
        }

        void resetState() {
            composingText = "";
        }

        @Override
        public CharSequence getTextBeforeCursor(int length, int flags) {
            return "";
        }

        @Override
        public CharSequence getTextAfterCursor(int length, int flags) {
            return "";
        }

        @Override
        public CharSequence getSelectedText(int flags) {
            return "";
        }

        @Override
        public boolean commitText(CharSequence text, int newCursorPosition) {
            String value = text == null ? "" : text.toString();
            if (value.length() == 0) {
                composingText = "";
                return true;
            }

            // If the IME previously delivered the same text through
            // setComposingText(), it is already visible in egui/winit. Treat the
            // commit as confirmation, not as new input, to avoid duplicates.
            if (composingText.length() > 0 && value.equals(composingText)) {
                composingText = "";
                return true;
            }

            composingText = "";
            return sendTextAsKeyEvents(value);
        }

        @Override
        public boolean setComposingText(CharSequence text, int newCursorPosition) {
            String value = text == null ? "" : text.toString();

            // Keep no hidden editable state. Mirror the IME's replacement of the
            // composing region by deleting the previously mirrored composition
            // from the app, then sending the new composition to the app.
            if (composingText.length() > 0) {
                sendBackspace(composingText.codePointCount(0, composingText.length()));
            }
            if (value.length() > 0) {
                sendTextAsKeyEvents(value);
            }
            composingText = value;
            return true;
        }

        @Override
        public boolean finishComposingText() {
            composingText = "";
            return true;
        }

        @Override
        public boolean deleteSurroundingText(int beforeLength, int afterLength) {
            if (beforeLength > 0) {
                sendBackspace(beforeLength);
                composingText = dropCodePointsFromEnd(composingText, beforeLength);
            } else if (afterLength > 0) {
                sendForwardDelete(afterLength);
            } else {
                sendBackspace(1);
                composingText = dropCodePointsFromEnd(composingText, 1);
            }
            return true;
        }

        @Override
        public boolean deleteSurroundingTextInCodePoints(int beforeLength, int afterLength) {
            return deleteSurroundingText(beforeLength, afterLength);
        }

        @Override
        public boolean sendKeyEvent(KeyEvent event) {
            if (event == null) {
                return true;
            }

            if (event.getAction() == KeyEvent.ACTION_DOWN) {
                switch (event.getKeyCode()) {
                    case KeyEvent.KEYCODE_DEL:
                        composingText = dropCodePointsFromEnd(composingText, 1);
                        break;
                    case KeyEvent.KEYCODE_FORWARD_DEL:
                    case KeyEvent.KEYCODE_ENTER:
                    case KeyEvent.KEYCODE_TAB:
                        composingText = "";
                        break;
                    default:
                        break;
                }
            }
            return super.sendKeyEvent(event);
        }

        @Override
        public boolean performEditorAction(int actionCode) {
            composingText = "";
            sendKeyPair(KeyEvent.KEYCODE_ENTER);
            return true;
        }

        private boolean sendTextAsKeyEvents(String text) {
            if (text.length() == 0) {
                return true;
            }

            // KeyCharacterMap covers Latin letters, digits and common symbols in
            // the same format NativeActivity/winit already understands. For
            // characters that cannot be represented as key events, fall back to
            // BaseInputConnection's dummy commit path.
            KeyEvent[] events = KeyCharacterMap.load(KeyCharacterMap.VIRTUAL_KEYBOARD)
                .getEvents(text.toCharArray());
            if (events != null) {
                for (KeyEvent event : events) {
                    super.sendKeyEvent(event);
                }
                return true;
            }

            return super.commitText(text, 1);
        }

        private void sendBackspace(int count) {
            int n = Math.max(1, count);
            for (int i = 0; i < n; i++) {
                sendKeyPair(KeyEvent.KEYCODE_DEL);
            }
        }

        private void sendForwardDelete(int count) {
            int n = Math.max(1, count);
            for (int i = 0; i < n; i++) {
                sendKeyPair(KeyEvent.KEYCODE_FORWARD_DEL);
            }
        }

        private void sendKeyPair(int keyCode) {
            long now = android.os.SystemClock.uptimeMillis();
            super.sendKeyEvent(new KeyEvent(now, now, KeyEvent.ACTION_DOWN, keyCode, 0));
            super.sendKeyEvent(new KeyEvent(now, now, KeyEvent.ACTION_UP, keyCode, 0));
        }

        private String dropCodePointsFromEnd(String value, int count) {
            if (value == null || value.length() == 0 || count <= 0) {
                return "";
            }
            int keep = value.offsetByCodePoints(value.length(), -Math.min(count, value.codePointCount(0, value.length())));
            return value.substring(0, keep);
        }
    }

    private static native void nativeOnBackPressed();
}
