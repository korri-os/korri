package org.korri.signer.test;

import android.app.Activity;
import android.content.Intent;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.widget.Button;
import android.widget.TextView;

import org.json.JSONObject;

/** Real NIP-55 Activity with configurable outcomes for emulator and physical proofs. */
public final class SignerActivity extends Activity {
    private final Handler handler = new Handler(Looper.getMainLooper());

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        handle(getIntent());
    }

    @Override
    protected void onNewIntent(Intent intent) {
        super.onNewIntent(intent);
        setIntent(intent);
        handle(intent);
    }

    private void handle(Intent intent) {
        if (!Intent.ACTION_VIEW.equals(intent.getAction())) {
            showConfiguration();
            return;
        }
        String type = intent.getStringExtra("type");
        if ("get_public_key".equals(type)) {
            SignerBehavior.selectAccount(this);
            Intent result = result(intent);
            result.putExtra("result", Bip340EventSigner.publicKey());
            result.putExtra("package", getPackageName());
            setResult(RESULT_OK, result);
            finish();
            return;
        }
        if (!"sign_event".equals(type)) {
            setResult(RESULT_CANCELED);
            finish();
            return;
        }
        Runnable answer = () -> answerSignEvent(intent, SignerBehavior.read(this));
        if (SignerBehavior.read(this) == SignerBehavior.Kind.DELAY) {
            handler.postDelayed(answer, 1_500L);
        } else {
            answer.run();
        }
    }

    private void answerSignEvent(Intent request, SignerBehavior.Kind behavior) {
        Intent result = result(request);
        if (behavior == SignerBehavior.Kind.DENY) {
            result.putExtra("rejected", true);
            setResult(RESULT_OK, result);
            finish();
            return;
        }
        try {
            String event;
            if (behavior == SignerBehavior.Kind.MALFORMED) {
                event = "{";
            } else {
                String template = request.getData() == null
                        ? null
                        : request.getData().getSchemeSpecificPart();
                event = Bip340EventSigner.signEvent(
                        template,
                        behavior == SignerBehavior.Kind.WRONG_EVENT);
            }
            result.putExtra("event", event);
            if (!"{".equals(event)) {
                result.putExtra("result", new JSONObject(event).getString("sig"));
            }
            setResult(RESULT_OK, result);
        } catch (Exception error) {
            setResult(RESULT_CANCELED);
        }
        finish();
    }

    private static Intent result(Intent request) {
        return new Intent().putExtra("id", request.getStringExtra("id"));
    }

    private void showConfiguration() {
        setContentView(R.layout.activity_signer);
        TextView status = findViewById(R.id.status);
        bind(R.id.approve, SignerBehavior.Kind.APPROVE, status);
        bind(R.id.deny, SignerBehavior.Kind.DENY, status);
        bind(R.id.delay, SignerBehavior.Kind.DELAY, status);
        bind(R.id.malformed, SignerBehavior.Kind.MALFORMED, status);
        bind(R.id.wrong_event, SignerBehavior.Kind.WRONG_EVENT, status);
        updateStatus(status);
        Button approve = findViewById(R.id.approve);
        approve.requestFocus();
    }

    private void bind(int buttonId, SignerBehavior.Kind behavior, TextView status) {
        findViewById(buttonId).setOnClickListener(view -> {
            SignerBehavior.write(this, behavior);
            updateStatus(status);
        });
    }

    private void updateStatus(TextView status) {
        status.setText("NIP-55 behavior: " + SignerBehavior.read(this).name());
    }
}
