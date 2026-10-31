package com.segrusa.aurelius_mobile

import android.content.ClipData
import android.content.ClipDescription
import android.content.ClipboardManager
import android.content.Context
import android.os.Build
import android.os.PersistableBundle
import android.view.WindowManager
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel
import java.util.UUID

class MainActivity : FlutterActivity() {
    /// A vault value shows (E19): FLAG_SECURE keeps it out of screenshots,
    /// recordings and the recents snapshot.
    private var wantSecure = false
    private var resumed = false

    override fun onResume() {
        super.onResume()
        resumed = true
        applySecure()
    }

    override fun onPause() {
        resumed = false
        super.onPause()
    }

    /// Off only while in the foreground: the recents snapshot is taken on the
    /// way out, before Flutter has drawn the value hidden.
    private fun applySecure() {
        if (wantSecure) {
            window.addFlags(WindowManager.LayoutParams.FLAG_SECURE)
        } else if (resumed) {
            window.clearFlags(WindowManager.LayoutParams.FLAG_SECURE)
        }
    }

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        // Vault values (E19). Never logs a value: a copy is marked with a
        // random id in the clip's extras, which tells later whether it is still ours.
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, "aurelius/secure").setMethodCallHandler { call, result ->
            val clipboard = getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
            when (call.method) {
                "setSecure" -> {
                    wantSecure = call.argument<Boolean>("on") == true
                    applySecure()
                    result.success(null)
                }
                "copySensitive" -> {
                    val text = call.argument<String>("text")
                    if (text == null) {
                        result.success(null)
                    } else {
                        val id = UUID.randomUUID().toString()
                        val clip = ClipData.newPlainText("Aurelius vault", text)
                        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.N) {
                            val extras = PersistableBundle()
                            // Android 13+ hides the preview of sensitive content.
                            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                                extras.putBoolean(ClipDescription.EXTRA_IS_SENSITIVE, true)
                            } else {
                                extras.putBoolean("android.content.extra.IS_SENSITIVE", true)
                            }
                            extras.putString(CLIP_MARK, id)
                            clip.description.extras = extras
                        }
                        clipboard.setPrimaryClip(clip)
                        result.success(id)
                    }
                }
                "clearIfOurs" -> {
                    // Android 10+ reads the clipboard only with focus: false = try again later.
                    if (!hasWindowFocus()) {
                        result.success(false)
                    } else {
                        val mark = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.N) clipboard.primaryClipDescription?.extras?.getString(CLIP_MARK) else null
                        if (mark != null && mark == call.argument<String>("id")) {
                            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
                                clipboard.clearPrimaryClip()
                            } else {
                                clipboard.setPrimaryClip(ClipData.newPlainText("", ""))
                            }
                        }
                        result.success(true)
                    }
                }
                else -> result.notImplemented()
            }
        }
        // Images on the clipboard: Flutter's text fields only paste text.
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, "aurelius/clipboard").setMethodCallHandler { call, result ->
            val clipboard = getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
            when (call.method) {
                "hasImage" -> result.success(clipboard.primaryClipDescription?.hasMimeType("image/*") == true)
                "image" -> {
                    val clip = clipboard.primaryClip
                    val uri = if (clip != null && clip.itemCount > 0) clip.getItemAt(0).uri else null
                    if (uri == null) {
                        result.success(null)
                    } else {
                        try {
                            val type = contentResolver.getType(uri) ?: "image/png"
                            val bytes = contentResolver.openInputStream(uri)?.use { it.readBytes() }
                            val extension = when (type) {
                                "image/jpeg" -> "jpg"
                                "image/gif" -> "gif"
                                "image/webp" -> "webp"
                                else -> "png"
                            }
                            result.success(if (bytes == null) null else mapOf("bytes" to bytes, "extension" to extension))
                        } catch (e: Exception) {
                            result.error("clipboard", e.message, null)
                        }
                    }
                }
                else -> result.notImplemented()
            }
        }
    }

    companion object {
        private const val CLIP_MARK = "com.segrusa.aurelius.vault_clip"
    }
}
