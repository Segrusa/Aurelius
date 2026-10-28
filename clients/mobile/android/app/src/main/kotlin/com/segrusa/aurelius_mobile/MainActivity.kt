package com.segrusa.aurelius_mobile

import android.content.ClipboardManager
import android.content.Context
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel

class MainActivity : FlutterActivity() {
    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
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
}
