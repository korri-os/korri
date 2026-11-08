package com.simonwjackson.korri.guidespike;

import android.app.Activity;
import android.os.Bundle;
import android.widget.TextView;

/** Exists only so the package leaves the stopped state; does nothing else. */
public class MainActivity extends Activity {
    @Override protected void onCreate(Bundle b) {
        super.onCreate(b);
        TextView t = new TextView(this);
        t.setText("Korri Guide spike #382. Enable the accessibility service, then press Guide.");
        t.setPadding(48, 48, 48, 48);
        setContentView(t);
    }
}
