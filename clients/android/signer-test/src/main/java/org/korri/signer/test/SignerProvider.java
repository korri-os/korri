package org.korri.signer.test;

import android.content.ContentProvider;
import android.content.ContentValues;
import android.database.Cursor;
import android.database.MatrixCursor;
import android.net.Uri;

import org.json.JSONObject;

/** NIP-55 ContentResolver path used after the test account permission is selected. */
public final class SignerProvider extends ContentProvider {
    @Override
    public boolean onCreate() {
        return true;
    }

    @Override
    public Cursor query(
            Uri uri,
            String[] projection,
            String selection,
            String[] selectionArgs,
            String sortOrder) {
        if (getContext() == null
                || !SignerBehavior.accountSelected(getContext())
                || projection == null
                || projection.length < 3
                || !Bip340EventSigner.publicKey().equals(projection[2])) {
            return null;
        }
        SignerBehavior.Kind behavior = SignerBehavior.read(getContext());
        if (behavior == SignerBehavior.Kind.DENY) {
            MatrixCursor rejected = new MatrixCursor(new String[] {"rejected"});
            rejected.addRow(new Object[] {"true"});
            return rejected;
        }
        if (behavior == SignerBehavior.Kind.DELAY) {
            try {
                Thread.sleep(1_500L);
            } catch (InterruptedException error) {
                Thread.currentThread().interrupt();
                return null;
            }
        }
        try {
            String event = behavior == SignerBehavior.Kind.MALFORMED
                    ? "{"
                    : Bip340EventSigner.signEvent(
                            projection[0],
                            behavior == SignerBehavior.Kind.WRONG_EVENT);
            String signature = "{".equals(event)
                    ? "malformed"
                    : new JSONObject(event).getString("sig");
            MatrixCursor approved = new MatrixCursor(new String[] {"result", "event"});
            approved.addRow(new Object[] {signature, event});
            return approved;
        } catch (Exception error) {
            return null;
        }
    }

    @Override public String getType(Uri uri) { return null; }
    @Override public Uri insert(Uri uri, ContentValues values) { return null; }
    @Override public int delete(Uri uri, String selection, String[] selectionArgs) { return 0; }
    @Override public int update(Uri uri, ContentValues values, String selection,
            String[] selectionArgs) { return 0; }
}
