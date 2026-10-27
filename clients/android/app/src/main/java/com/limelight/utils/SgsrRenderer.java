package com.limelight.utils;

import android.graphics.SurfaceTexture;
import android.opengl.GLES20;
import android.opengl.GLES30;
import android.opengl.GLSurfaceView;
import android.view.Surface;

import com.limelight.LimeLog;
import com.limelight.preferences.PreferenceConfiguration;
import com.limelight.ui.OnRenderSurfaceReadyListener;

import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.FloatBuffer;
import java.util.concurrent.atomic.AtomicBoolean;

import javax.microedition.khronos.egl.EGLConfig;
import javax.microedition.khronos.opengles.GL10;

/**
 * SPIKE: Snapdragon Game Super Resolution 1 (SGSR1) upscaling renderer.
 *
 * Pipeline: MediaCodec decodes into a SurfaceTexture (external OES texture at
 * stream resolution) -> pass 0 blits OES into a regular RGBA texture (SGSR
 * needs textureGather, which external textures do not support) -> pass 1 runs
 * Qualcomm's single-pass SGSR1 shader to the display-resolution framebuffer.
 *
 * SGSR1 is BSD-3 (Qualcomm Innovation Center); the fragment shader below is
 * their reference GLSL with only I/O plumbing adapted. Despite the branding it
 * is vendor-neutral GLSL ES 3.1 and runs fine on Mali/Immortalis.
 *
 * Reports its decoder surface through OnRenderSurfaceReadyListener, the
 * container-owned seam shared by every GL render mode.
 */
public class SgsrRenderer implements GLSurfaceView.Renderer, SurfaceTexture.OnFrameAvailableListener {
    private static final int GL_TEXTURE_EXTERNAL_OES = 0x8D65;

    private static final float[] QUAD_VERTICES = {-1.0f, -1.0f, 1.0f, -1.0f, -1.0f, 1.0f, 1.0f, 1.0f};
    // Identity texcoords. Orientation is handled once, in the blit pass, by
    // SurfaceTexture's transform matrix — not by hand-flipped coordinates.
    private static final float[] TEXTURE_VERTICES = {0.0f, 0.0f, 1.0f, 0.0f, 0.0f, 1.0f, 1.0f, 1.0f};

    private static final String BLIT_VERTEX_SHADER =
            "#version 300 es\n" +
            "layout(location=0) in vec4 a_Position;\n" +
            "layout(location=1) in vec2 a_TexCoord;\n" +
            "uniform mat4 u_TexMatrix;\n" +
            "out vec2 v_TexCoord;\n" +
            "void main() {\n" +
            "    gl_Position = a_Position;\n" +
            "    v_TexCoord = (u_TexMatrix * vec4(a_TexCoord, 0.0, 1.0)).xy;\n" +
            "}\n";

    private static final String BLIT_FRAGMENT_SHADER =
            "#version 300 es\n" +
            "#extension GL_OES_EGL_image_external_essl3 : require\n" +
            "precision mediump float;\n" +
            "in vec2 v_TexCoord;\n" +
            "uniform samplerExternalOES u_Texture;\n" +
            "out vec4 fragColor;\n" +
            "void main() {\n" +
            "    fragColor = texture(u_Texture, v_TexCoord);\n" +
            "}\n";

    private static final String SGSR_VERTEX_SHADER =
            "#version 310 es\n" +
            "layout(location=0) in vec4 a_Position;\n" +
            "layout(location=1) in vec2 a_TexCoord;\n" +
            "layout(location=0) out highp vec4 in_TEXCOORD0;\n" +
            "void main() {\n" +
            "    gl_Position = a_Position;\n" +
            "    in_TEXCOORD0 = vec4(a_TexCoord, 0.0, 0.0);\n" +
            "}\n";

    /**
     * Qualcomm SGSR1 mobile fragment shader (BSD-3-Clause,
     * Copyright (c) 2025 Qualcomm Innovation Center, Inc.).
     * OperationMode 1 (RGBA); ViewportInfo = (1/w, 1/h, w, h) of the INPUT.
     */
    private static final String SGSR_FRAGMENT_SHADER =
            "#version 310 es\n" +
            "precision mediump float;\n" +
            "precision highp int;\n" +
            "uniform float u_EdgeThreshold;\n" +
            "uniform float u_EdgeSharpness;\n" +
            "uniform highp vec4 ViewportInfo[1];\n" +
            "uniform mediump sampler2D ps0;\n" +
            "layout(location=0) in highp vec4 in_TEXCOORD0;\n" +
            "layout(location=0) out vec4 out_Target0;\n" +
            "float fastLanczos2(float x)\n" +
            "{\n" +
            "	float wA = x-4.0;\n" +
            "	float wB = x*wA-wA;\n" +
            "	wA *= wA;\n" +
            "	return wB*wA;\n" +
            "}\n" +
            "vec2 weightY(float dx, float dy,float c, float std)\n" +
            "{\n" +
            "	float x = ((dx*dx)+(dy* dy))* 0.55 + clamp(abs(c)*std, 0.0, 1.0);\n" +
            "	float w = fastLanczos2(x);\n" +
            "	return vec2(w, w * c);\n" +
            "}\n" +
            // Specialized for OperationMode 1 (RGBA): Mali requires the
            // textureGather component argument to be a literal constant, so
            // the reference shader's runtime `mode` variable is folded away
            // (component 1 == green, color[1] == color.y).
            "void main()\n" +
            "{\n" +
            "	float edgeThreshold = u_EdgeThreshold;\n" +
            "	float edgeSharpness = u_EdgeSharpness;\n" +
            "	vec4 color;\n" +
            "	color.xyz = textureLod(ps0,in_TEXCOORD0.xy,0.0).xyz;\n" +
            "	{\n" +
            "		highp vec2 imgCoord = ((in_TEXCOORD0.xy*ViewportInfo[0].zw)+vec2(-0.5,0.5));\n" +
            "		highp vec2 imgCoordPixel = floor(imgCoord);\n" +
            "		highp vec2 coord = (imgCoordPixel*ViewportInfo[0].xy);\n" +
            "		vec2 pl = (imgCoord+(-imgCoordPixel));\n" +
            "		vec4  left = textureGather(ps0,coord, 1);\n" +
            "		float edgeVote = abs(left.z - left.y) + abs(color.y - left.y)  + abs(color.y - left.z) ;\n" +
            "		if(edgeVote > edgeThreshold)\n" +
            "		{\n" +
            "			coord.x += ViewportInfo[0].x;\n" +
            "			vec4 right = textureGather(ps0,coord + vec2(ViewportInfo[0].x, 0.0), 1);\n" +
            "			vec4 upDown;\n" +
            "			upDown.xy = textureGather(ps0,coord + vec2(0.0, -ViewportInfo[0].y),1).wz;\n" +
            "			upDown.zw  = textureGather(ps0,coord+ vec2(0.0, ViewportInfo[0].y), 1).yx;\n" +
            "			float mean = (left.y+left.z+right.x+right.w)*0.25;\n" +
            "			left = left - vec4(mean);\n" +
            "			right = right - vec4(mean);\n" +
            "			upDown = upDown - vec4(mean);\n" +
            "			color.w =color.y - mean;\n" +
            "			float sum = (((((abs(left.x)+abs(left.y))+abs(left.z))+abs(left.w))+(((abs(right.x)+abs(right.y))+abs(right.z))+abs(right.w)))+(((abs(upDown.x)+abs(upDown.y))+abs(upDown.z))+abs(upDown.w)));\n" +
            "			float std = 2.181818/sum;\n" +
            "			vec2 aWY = weightY(pl.x, pl.y+1.0, upDown.x,std);\n" +
            "			aWY += weightY(pl.x-1.0, pl.y+1.0, upDown.y,std);\n" +
            "			aWY += weightY(pl.x-1.0, pl.y-2.0, upDown.z,std);\n" +
            "			aWY += weightY(pl.x, pl.y-2.0, upDown.w,std);\n" +
            "			aWY += weightY(pl.x+1.0, pl.y-1.0, left.x,std);\n" +
            "			aWY += weightY(pl.x, pl.y-1.0, left.y,std);\n" +
            "			aWY += weightY(pl.x, pl.y, left.z,std);\n" +
            "			aWY += weightY(pl.x+1.0, pl.y, left.w,std);\n" +
            "			aWY += weightY(pl.x-1.0, pl.y-1.0, right.x,std);\n" +
            "			aWY += weightY(pl.x-2.0, pl.y-1.0, right.y,std);\n" +
            "			aWY += weightY(pl.x-2.0, pl.y, right.z,std);\n" +
            "			aWY += weightY(pl.x-1.0, pl.y, right.w,std);\n" +
            "			float finalY = aWY.y/aWY.x;\n" +
            "			float maxY = max(max(left.y,left.z),max(right.x,right.w));\n" +
            "			float minY = min(min(left.y,left.z),min(right.x,right.w));\n" +
            "			finalY = clamp(edgeSharpness*finalY, minY, maxY);\n" +
            "			float deltaY = finalY -color.w;\n" +
            "			deltaY = clamp(deltaY, -23.0 / 255.0, 23.0 / 255.0);\n" +
            "			color.x = clamp((color.x+deltaY),0.0,1.0);\n" +
            "			color.y = clamp((color.y+deltaY),0.0,1.0);\n" +
            "			color.z = clamp((color.z+deltaY),0.0,1.0);\n" +
            "		}\n" +
            "	}\n" +
            "	color.w = 1.0;\n" +
            "	out_Target0.xyzw = color;\n" +
            "}\n";

    private final GLSurfaceView glSurfaceView;
    private final OnRenderSurfaceReadyListener onSurfaceReadyListener;
    private final int sourceWidth;
    private final int sourceHeight;
    private volatile float edgeSharpness;
    private volatile float edgeThreshold;

    private final FloatBuffer quadVertexBuffer;
    private final FloatBuffer textureVertexBuffer;
    private final AtomicBoolean frameAvailable = new AtomicBoolean(false);

    private SurfaceTexture videoSurfaceTexture;
    private Surface videoSurface;
    private final float[] texMatrix = new float[16];

    private int videoTextureId;
    private int sourceTextureId;
    private int sourceFboHandle;
    private int blitProgram;
    private int sgsrProgram;
    private int outputWidth;
    private int outputHeight;

    public SgsrRenderer(GLSurfaceView view,
                        OnRenderSurfaceReadyListener listener,
                        PreferenceConfiguration prefConfig) {
        this.glSurfaceView = view;
        this.onSurfaceReadyListener = listener;
        this.sourceWidth = prefConfig.width;
        this.sourceHeight = prefConfig.height;
        this.edgeSharpness = prefConfig.sgsrSharpness;
        this.edgeThreshold = prefConfig.sgsrEdgeThreshold;
        // Uniforms are re-read from these fields every frame, so
        // updateEdgeParams() takes effect live mid-stream.

        quadVertexBuffer = ByteBuffer.allocateDirect(QUAD_VERTICES.length * 4)
                .order(ByteOrder.nativeOrder()).asFloatBuffer();
        quadVertexBuffer.put(QUAD_VERTICES).position(0);
        textureVertexBuffer = ByteBuffer.allocateDirect(TEXTURE_VERTICES.length * 4)
                .order(ByteOrder.nativeOrder()).asFloatBuffer();
        textureVertexBuffer.put(TEXTURE_VERTICES).position(0);
    }

    @Override
    public void onFrameAvailable(SurfaceTexture surfaceTexture) {
        frameAvailable.set(true);
        glSurfaceView.requestRender();
    }

    @Override
    public void onSurfaceCreated(GL10 gl, EGLConfig config) {
        videoTextureId = createExternalOesTexture();
        videoSurfaceTexture = new SurfaceTexture(videoTextureId);
        videoSurfaceTexture.setOnFrameAvailableListener(this);
        videoSurface = new Surface(videoSurfaceTexture);

        blitProgram = createProgram(BLIT_VERTEX_SHADER, BLIT_FRAGMENT_SHADER);
        sgsrProgram = createProgram(SGSR_VERTEX_SHADER, SGSR_FRAGMENT_SHADER);

        sourceTextureId = createRgbaTexture(sourceWidth, sourceHeight);
        int[] fbos = new int[1];
        GLES20.glGenFramebuffers(1, fbos, 0);
        sourceFboHandle = fbos[0];
        GLES20.glBindFramebuffer(GLES20.GL_FRAMEBUFFER, sourceFboHandle);
        GLES20.glFramebufferTexture2D(GLES20.GL_FRAMEBUFFER, GLES20.GL_COLOR_ATTACHMENT0,
                GLES20.GL_TEXTURE_2D, sourceTextureId, 0);
        if (GLES20.glCheckFramebufferStatus(GLES20.GL_FRAMEBUFFER)
                != GLES20.GL_FRAMEBUFFER_COMPLETE) {
            LimeLog.severe("SGSR source framebuffer is not complete");
        }
        GLES20.glBindFramebuffer(GLES20.GL_FRAMEBUFFER, 0);

        LimeLog.info("SGSR renderer ready: " + sourceWidth + "x" + sourceHeight
                + " -> display, GL_RENDERER=" + GLES20.glGetString(GLES20.GL_RENDERER));

        onSurfaceReadyListener.onRenderSurfaceReady(videoSurface);
    }

    @Override
    public void onSurfaceChanged(GL10 gl, int width, int height) {
        outputWidth = width;
        outputHeight = height;
    }

    @Override
    public void onDrawFrame(GL10 gl) {
        if (!frameAvailable.getAndSet(false)) {
            return;
        }
        try {
            videoSurfaceTexture.updateTexImage();
            videoSurfaceTexture.getTransformMatrix(texMatrix);
        } catch (Exception e) {
            LimeLog.warning("SGSR updateTexImage failed: " + e);
            return;
        }

        // Pass 0: external OES -> regular RGBA texture at stream resolution.
        GLES20.glBindFramebuffer(GLES20.GL_FRAMEBUFFER, sourceFboHandle);
        GLES20.glViewport(0, 0, sourceWidth, sourceHeight);
        GLES20.glUseProgram(blitProgram);
        GLES20.glActiveTexture(GLES20.GL_TEXTURE0);
        GLES20.glBindTexture(GL_TEXTURE_EXTERNAL_OES, videoTextureId);
        GLES20.glUniform1i(GLES20.glGetUniformLocation(blitProgram, "u_Texture"), 0);
        GLES20.glUniformMatrix4fv(GLES20.glGetUniformLocation(blitProgram, "u_TexMatrix"),
                1, false, texMatrix, 0);
        drawQuad(blitProgram);

        // Pass 1: SGSR upscale + sharpen to the display framebuffer.
        GLES20.glBindFramebuffer(GLES20.GL_FRAMEBUFFER, 0);
        GLES20.glViewport(0, 0, outputWidth, outputHeight);
        GLES20.glUseProgram(sgsrProgram);
        GLES20.glActiveTexture(GLES20.GL_TEXTURE0);
        GLES20.glBindTexture(GLES20.GL_TEXTURE_2D, sourceTextureId);
        GLES20.glUniform1i(GLES20.glGetUniformLocation(sgsrProgram, "ps0"), 0);
        GLES20.glUniform4f(GLES20.glGetUniformLocation(sgsrProgram, "ViewportInfo[0]"),
                1.0f / sourceWidth, 1.0f / sourceHeight, sourceWidth, sourceHeight);
        GLES20.glUniform1f(GLES20.glGetUniformLocation(sgsrProgram, "u_EdgeSharpness"),
                edgeSharpness);
        GLES20.glUniform1f(GLES20.glGetUniformLocation(sgsrProgram, "u_EdgeThreshold"),
                edgeThreshold);
        drawQuad(sgsrProgram);
    }

    /** Live-tunable from the in-game overlay; applied on the next frame. */
    public void updateEdgeParams(float sharpness, float threshold) {
        this.edgeSharpness = sharpness;
        this.edgeThreshold = threshold;
    }

    public void onSurfaceDestroyed() {
        if (videoSurface != null) {
            videoSurface.release();
            videoSurface = null;
        }
        if (videoSurfaceTexture != null) {
            videoSurfaceTexture.release();
            videoSurfaceTexture = null;
        }
        glSurfaceView.queueEvent(() -> {
            GLES20.glDeleteProgram(blitProgram);
            GLES20.glDeleteProgram(sgsrProgram);
            int[] textures = {videoTextureId, sourceTextureId};
            GLES20.glDeleteTextures(textures.length, textures, 0);
            int[] fbos = {sourceFboHandle};
            GLES20.glDeleteFramebuffers(fbos.length, fbos, 0);
        });
    }

    private void drawQuad(int program) {
        int positionHandle = GLES20.glGetAttribLocation(program, "a_Position");
        int texCoordHandle = GLES20.glGetAttribLocation(program, "a_TexCoord");
        GLES20.glEnableVertexAttribArray(positionHandle);
        GLES20.glVertexAttribPointer(positionHandle, 2, GLES20.GL_FLOAT, false, 0, quadVertexBuffer);
        GLES20.glEnableVertexAttribArray(texCoordHandle);
        GLES20.glVertexAttribPointer(texCoordHandle, 2, GLES20.GL_FLOAT, false, 0, textureVertexBuffer);
        GLES20.glDrawArrays(GLES20.GL_TRIANGLE_STRIP, 0, 4);
        GLES20.glDisableVertexAttribArray(positionHandle);
        GLES20.glDisableVertexAttribArray(texCoordHandle);
    }

    private int createExternalOesTexture() {
        int[] textures = new int[1];
        GLES20.glGenTextures(1, textures, 0);
        GLES20.glBindTexture(GL_TEXTURE_EXTERNAL_OES, textures[0]);
        GLES20.glTexParameteri(GL_TEXTURE_EXTERNAL_OES, GLES20.GL_TEXTURE_MIN_FILTER, GLES20.GL_LINEAR);
        GLES20.glTexParameteri(GL_TEXTURE_EXTERNAL_OES, GLES20.GL_TEXTURE_MAG_FILTER, GLES20.GL_LINEAR);
        GLES20.glTexParameteri(GL_TEXTURE_EXTERNAL_OES, GLES20.GL_TEXTURE_WRAP_S, GLES20.GL_CLAMP_TO_EDGE);
        GLES20.glTexParameteri(GL_TEXTURE_EXTERNAL_OES, GLES20.GL_TEXTURE_WRAP_T, GLES20.GL_CLAMP_TO_EDGE);
        return textures[0];
    }

    private int createRgbaTexture(int width, int height) {
        int[] textures = new int[1];
        GLES20.glGenTextures(1, textures, 0);
        GLES20.glBindTexture(GLES20.GL_TEXTURE_2D, textures[0]);
        GLES20.glTexImage2D(GLES20.GL_TEXTURE_2D, 0, GLES20.GL_RGBA, width, height, 0,
                GLES20.GL_RGBA, GLES20.GL_UNSIGNED_BYTE, null);
        GLES20.glTexParameteri(GLES20.GL_TEXTURE_2D, GLES20.GL_TEXTURE_MIN_FILTER, GLES20.GL_LINEAR);
        GLES20.glTexParameteri(GLES20.GL_TEXTURE_2D, GLES20.GL_TEXTURE_MAG_FILTER, GLES20.GL_LINEAR);
        GLES20.glTexParameteri(GLES20.GL_TEXTURE_2D, GLES20.GL_TEXTURE_WRAP_S, GLES20.GL_CLAMP_TO_EDGE);
        GLES20.glTexParameteri(GLES20.GL_TEXTURE_2D, GLES20.GL_TEXTURE_WRAP_T, GLES20.GL_CLAMP_TO_EDGE);
        return textures[0];
    }

    private int createProgram(String vertexSource, String fragmentSource) {
        int vertexShader = compileShader(GLES20.GL_VERTEX_SHADER, vertexSource);
        int fragmentShader = compileShader(GLES20.GL_FRAGMENT_SHADER, fragmentSource);
        int program = GLES20.glCreateProgram();
        GLES20.glAttachShader(program, vertexShader);
        GLES20.glAttachShader(program, fragmentShader);
        GLES20.glLinkProgram(program);
        int[] linkStatus = new int[1];
        GLES20.glGetProgramiv(program, GLES20.GL_LINK_STATUS, linkStatus, 0);
        if (linkStatus[0] == 0) {
            LimeLog.severe("SGSR program link failed: " + GLES20.glGetProgramInfoLog(program));
        }
        GLES20.glDeleteShader(vertexShader);
        GLES20.glDeleteShader(fragmentShader);
        return program;
    }

    private int compileShader(int type, String source) {
        int shader = GLES20.glCreateShader(type);
        GLES20.glShaderSource(shader, source);
        GLES20.glCompileShader(shader);
        int[] compiled = new int[1];
        GLES20.glGetShaderiv(shader, GLES20.GL_COMPILE_STATUS, compiled, 0);
        if (compiled[0] == 0) {
            LimeLog.severe("SGSR shader compile failed: " + GLES20.glGetShaderInfoLog(shader));
        }
        return shader;
    }
}
