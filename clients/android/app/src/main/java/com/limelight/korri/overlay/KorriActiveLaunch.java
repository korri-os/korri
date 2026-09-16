package com.limelight.korri.overlay;

import android.content.ComponentName;

import org.json.JSONArray;
import org.json.JSONObject;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.Objects;

/** Immutable process-local mirror of korrid's signed Android launch context. */
public final class KorriActiveLaunch {
    public interface ThrowingStart {
        void run() throws Exception;
    }

    public interface Record {
        void run() throws Exception;
    }

    private static final class Contributor {
        final String kind;
        final String id;

        Contributor(String kind, String id) {
            this.kind = kind;
            this.id = id;
        }
    }

    private final String launchId;
    private final String gameId;
    private final String title;
    private final List<Contributor> contributors;
    private final String executorId;
    private final boolean executorAvailable;
    private final String foregroundKind;
    private final String foregroundPackage;
    private final String foregroundClass;

    private KorriActiveLaunch(
            String launchId,
            String gameId,
            String title,
            List<Contributor> contributors,
            String executorId,
            boolean executorAvailable,
            String foregroundKind,
            String foregroundPackage,
            String foregroundClass) {
        this.launchId = launchId;
        this.gameId = gameId;
        this.title = title;
        this.contributors = Collections.unmodifiableList(new ArrayList<>(contributors));
        this.executorId = executorId;
        this.executorAvailable = executorAvailable;
        this.foregroundKind = foregroundKind;
        this.foregroundPackage = foregroundPackage;
        this.foregroundClass = foregroundClass;
    }

    public static KorriActiveLaunch fromJson(String json) throws Exception {
        JSONObject value = new JSONObject(json);
        String launchId = required(value, "launchId");
        if (launchId.length() != 32) {
            throw new IllegalArgumentException("launchId must be the korrid session identity");
        }
        JSONArray contributorValues = value.getJSONArray("contributors");
        List<Contributor> contributors = new ArrayList<>();
        for (int index = 0; index < contributorValues.length(); index++) {
            JSONObject contributor = contributorValues.getJSONObject(index);
            contributors.add(new Contributor(
                    required(contributor, "kind"), required(contributor, "id")));
        }
        JSONObject executor = value.optJSONObject("executor");
        JSONObject foreground = value.getJSONObject("foreground");
        String kind = required(foreground, "kind");
        if (!"component".equals(kind) && !"package".equals(kind)) {
            throw new IllegalArgumentException("unknown foreground match rule");
        }
        return new KorriActiveLaunch(
                launchId,
                nullable(value, "gameId"),
                nullable(value, "title"),
                contributors,
                executor == null ? null : required(executor, "id"),
                executor != null && executor.getBoolean("available"),
                kind,
                required(foreground, "packageName"),
                "component".equals(kind) ? required(foreground, "className") : null);
    }

    public static KorriActiveLaunch artemis(
            String launchId,
            String gameId,
            String title,
            String applicationPackage,
            ComponentName gameComponent,
            String transportId,
            String executorId,
            boolean executorAvailable) {
        if (!applicationPackage.equals(gameComponent.getPackageName())) {
            throw new IllegalArgumentException("Artemis Game must belong to this application");
        }
        return new KorriActiveLaunch(
                launchId,
                gameId,
                title,
                Collections.singletonList(new Contributor("transport", transportId)),
                executorId,
                executorAvailable,
                "component",
                applicationPackage,
                gameComponent.getClassName());
    }

    public static KorriActiveLaunch packageLaunch(
            String launchId,
            String gameId,
            String title,
            String packageName,
            String runnerId) {
        return new KorriActiveLaunch(
                launchId,
                gameId,
                title,
                Collections.singletonList(new Contributor("runner", runnerId)),
                null,
                false,
                "package",
                packageName,
                null);
    }

    /** The launch record is not visible until Android accepted the start. */
    public static void startThenRecord(ThrowingStart start, Record record) throws Exception {
        start.run();
        record.run();
    }

    public String launchId() {
        return launchId;
    }

    public String gameId() {
        return gameId;
    }

    public String title() {
        return title;
    }

    public boolean executorAvailable() {
        return executorAvailable;
    }

    public boolean requiresExecutorPublication() {
        return "android-moonlight".equals(executorId);
    }

    public List<String> contributorKeys() {
        List<String> keys = new ArrayList<>();
        for (Contributor contributor : contributors) {
            keys.add(contributor.kind + ":" + contributor.id);
        }
        return Collections.unmodifiableList(keys);
    }

    public String targetPackage() {
        return foregroundPackage;
    }

    public boolean matchesForeground(String packageName, String className) {
        if (!foregroundPackage.equals(packageName)) {
            return false;
        }
        return "package".equals(foregroundKind)
                || Objects.equals(foregroundClass, normalizeClass(packageName, className));
    }

    public String toJson() {
        try {
            JSONObject value = new JSONObject()
                    .put("launchId", launchId)
                    .put("gameId", gameId == null ? JSONObject.NULL : gameId)
                    .put("title", title == null ? JSONObject.NULL : title);
            JSONArray ordered = new JSONArray();
            for (Contributor contributor : contributors) {
                ordered.put(new JSONObject()
                        .put("kind", contributor.kind)
                        .put("id", contributor.id));
            }
            value.put("contributors", ordered);
            value.put("executor", executorId == null
                    ? JSONObject.NULL
                    : new JSONObject().put("id", executorId)
                            .put("available", executorAvailable));
            JSONObject foreground = new JSONObject()
                    .put("kind", foregroundKind)
                    .put("packageName", foregroundPackage);
            if (foregroundClass != null) {
                foreground.put("className", foregroundClass);
            }
            return value.put("foreground", foreground).toString();
        } catch (Exception error) {
            throw new IllegalStateException(error);
        }
    }

    private static String normalizeClass(String packageName, String className) {
        if (className == null) return null;
        return className.startsWith(".") ? packageName + className : className;
    }

    private static String required(JSONObject value, String key) throws Exception {
        String result = value.getString(key);
        if (result.isEmpty()) throw new IllegalArgumentException(key + " is empty");
        return result;
    }

    private static String nullable(JSONObject value, String key) {
        return value.isNull(key) ? null : value.optString(key, null);
    }
}
