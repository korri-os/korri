package org.korri.signer.test;

import org.bouncycastle.asn1.sec.SECNamedCurves;
import org.bouncycastle.asn1.x9.X9ECParameters;
import org.bouncycastle.math.ec.ECPoint;
import org.json.JSONArray;
import org.json.JSONObject;

import java.math.BigInteger;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.util.Arrays;

/** Small deterministic BIP-340 signer used only by the configurable test application. */
final class Bip340EventSigner {
    private static final X9ECParameters CURVE = SECNamedCurves.getByName("secp256k1");
    private static final BigInteger N = CURVE.getN();
    private static final BigInteger PRIVATE_KEY = BigInteger.valueOf(3L);

    static String publicKey() {
        return hex(pointX(CURVE.getG().multiply(normalizedSecret()).normalize()));
    }

    static String signEvent(String unsignedEventJson, boolean wrongEvent) throws Exception {
        JSONObject template = new JSONObject(unsignedEventJson);
        long createdAt = template.getLong("created_at");
        int kind = template.getInt("kind");
        JSONArray tags = template.getJSONArray("tags");
        String content = template.getString("content");
        if (wrongEvent) content = content + "wrong";

        JSONArray serialized = new JSONArray();
        serialized.put(0);
        serialized.put(publicKey());
        serialized.put(createdAt);
        serialized.put(kind);
        serialized.put(tags);
        serialized.put(content);
        byte[] id = sha256(serialized.toString().getBytes(StandardCharsets.UTF_8));
        byte[] signature = sign(id);

        JSONObject event = new JSONObject();
        event.put("id", hex(id));
        event.put("pubkey", publicKey());
        event.put("created_at", createdAt);
        event.put("kind", kind);
        event.put("tags", tags);
        event.put("content", content);
        event.put("sig", hex(signature));
        return event.toString();
    }

    private static byte[] sign(byte[] message) throws Exception {
        BigInteger d = normalizedSecret();
        ECPoint publicPoint = CURVE.getG().multiply(d).normalize();
        byte[] publicX = pointX(publicPoint);
        byte[] auxiliary = new byte[32];
        byte[] t = xor(to32(d), taggedHash("BIP0340/aux", auxiliary));
        BigInteger nonce = new BigInteger(1, taggedHash(
                "BIP0340/nonce", concat(t, publicX, message))).mod(N);
        if (nonce.signum() == 0) throw new IllegalStateException("zero nonce");
        ECPoint rPoint = CURVE.getG().multiply(nonce).normalize();
        if (rPoint.getAffineYCoord().toBigInteger().testBit(0)) {
            nonce = N.subtract(nonce);
            rPoint = CURVE.getG().multiply(nonce).normalize();
        }
        byte[] r = pointX(rPoint);
        BigInteger challenge = new BigInteger(1, taggedHash(
                "BIP0340/challenge", concat(r, publicX, message))).mod(N);
        BigInteger s = nonce.add(challenge.multiply(d)).mod(N);
        return concat(r, to32(s));
    }

    private static BigInteger normalizedSecret() {
        ECPoint point = CURVE.getG().multiply(PRIVATE_KEY).normalize();
        return point.getAffineYCoord().toBigInteger().testBit(0)
                ? N.subtract(PRIVATE_KEY)
                : PRIVATE_KEY;
    }

    private static byte[] taggedHash(String tag, byte[] data) throws Exception {
        byte[] tagHash = sha256(tag.getBytes(StandardCharsets.UTF_8));
        return sha256(concat(tagHash, tagHash, data));
    }

    private static byte[] sha256(byte[] value) throws Exception {
        return MessageDigest.getInstance("SHA-256").digest(value);
    }

    private static byte[] pointX(ECPoint point) {
        return to32(point.getAffineXCoord().toBigInteger());
    }

    private static byte[] to32(BigInteger value) {
        byte[] encoded = value.toByteArray();
        byte[] result = new byte[32];
        int source = Math.max(0, encoded.length - 32);
        int length = Math.min(32, encoded.length);
        System.arraycopy(encoded, source, result, 32 - length, length);
        return result;
    }

    private static byte[] concat(byte[]... values) {
        int length = Arrays.stream(values).mapToInt(value -> value.length).sum();
        byte[] result = new byte[length];
        int offset = 0;
        for (byte[] value : values) {
            System.arraycopy(value, 0, result, offset, value.length);
            offset += value.length;
        }
        return result;
    }

    private static byte[] xor(byte[] left, byte[] right) {
        byte[] result = new byte[left.length];
        for (int index = 0; index < left.length; index++) {
            result[index] = (byte) (left[index] ^ right[index]);
        }
        return result;
    }

    private static String hex(byte[] value) {
        StringBuilder result = new StringBuilder(value.length * 2);
        for (byte item : value) result.append(String.format("%02x", item & 0xff));
        return result.toString();
    }

    private Bip340EventSigner() {}
}
