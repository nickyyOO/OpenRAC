// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) 2026 the OpenRAC contributors

package dev.openrac.launcher

import android.app.ActivityManager
import android.app.ApplicationExitInfo
import android.content.Context
import android.os.Build
import android.util.Log
import java.io.File
import java.io.PrintWriter
import java.io.StringWriter
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

/**
 * Crash logs a player can send without adb: files/logs, "Crash logs" in the Files app
 * (FilesProvider.kt). The game writes its own reports when it can (port/common/crash.h); this
 * catches what it cannot: a crash of the launcher, a Java crash, a kill for memory, a hang (ANR),
 * a crash inside a driver before the game's handler is set.
 */
object ExitReports {
  private const val TAG = "OpenRAC"
  private const val KEEP = 20

  fun folder(context: Context): File = File(context.filesDir, "logs")

  /** Writes a Java crash of this process to files/logs, then lets Android end it as before. */
  @JvmStatic
  fun catchUncaught(context: Context) {
    val app = context.applicationContext
    val before = Thread.getDefaultUncaughtExceptionHandler()
    Thread.setDefaultUncaughtExceptionHandler { thread, error ->
      try {
        val trace = StringWriter().also { error.printStackTrace(PrintWriter(it)) }.toString()
        write(app, "java-crash", header(app, "Java crash in thread ${thread.name}") + "\n" + trace, null)
      } catch (e: Throwable) {
        // Nothing more can be done in a crashing process.
      }
      before?.uncaughtException(thread, error)
    }
  }

  /** Writes a file for each crash, hang or kill Android recorded since the last call (Android 11
   *  and later keep these for the app). Runs off the main thread. */
  fun collect(context: Context) {
    if (Build.VERSION.SDK_INT < 30) return
    val app = context.applicationContext
    Thread {
      try {
        collectNow(app)
      } catch (e: Exception) {
        Log.w(TAG, "cannot read the app's exit reasons: $e")
      }
    }.start()
  }

  @Synchronized
  private fun collectNow(context: Context) {
    if (Build.VERSION.SDK_INT < 30) return
    val dir = folder(context)
    dir.mkdirs()
    val seenFile = File(dir, ".seen")
    val seen = seenFile.takeIf { it.isFile }?.readText()?.trim()?.toLongOrNull() ?: 0L
    val manager = context.getSystemService(ActivityManager::class.java)
    var newest = seen
    for (exit in manager.getHistoricalProcessExitReasons(null, 0, 16)) {
      if (exit.timestamp <= seen) continue
      newest = maxOf(newest, exit.timestamp)
      val kind = kind(exit.reason) ?: continue
      val text = StringBuilder(header(context, "${reasonName(exit.reason)} of ${exit.processName}"))
      text.append("time       ${stamp(exit.timestamp)}\n")
      text.append("status     ${exit.status}\n")
      text.append("importance ${exit.importance}\n")
      text.append("memory     pss ${exit.pss} kB, rss ${exit.rss} kB\n")
      exit.description?.let { text.append("details    $it\n") }
      var tombstone: ByteArray? = null
      try {
        exit.traceInputStream?.use { stream ->
          val bytes = stream.readBytes()
          // An ANR's trace is text; a native crash's (Android 12 and later) is a tombstone proto.
          if (exit.reason == ApplicationExitInfo.REASON_ANR) {
            text.append("\n").append(bytes.toString(Charsets.UTF_8))
          } else {
            tombstone = bytes
          }
        }
      } catch (e: Exception) {
        text.append("trace      not readable: $e\n")
      }
      write(context, kind, text.toString(), tombstone, exit.timestamp)
    }
    seenFile.writeText(newest.toString())
    prune(dir)
  }

  private fun kind(reason: Int): String? = when (reason) {
    ApplicationExitInfo.REASON_CRASH -> "java-crash"
    ApplicationExitInfo.REASON_CRASH_NATIVE -> "native-crash"
    ApplicationExitInfo.REASON_ANR -> "hang"
    ApplicationExitInfo.REASON_LOW_MEMORY -> "low-memory"
    ApplicationExitInfo.REASON_EXCESSIVE_RESOURCE_USAGE -> "too-much-resources"
    ApplicationExitInfo.REASON_INITIALIZATION_FAILURE -> "start-failed"
    else -> null
  }

  private fun reasonName(reason: Int): String = when (reason) {
    ApplicationExitInfo.REASON_CRASH -> "Java crash"
    ApplicationExitInfo.REASON_CRASH_NATIVE -> "Native crash"
    ApplicationExitInfo.REASON_ANR -> "Hang (ANR)"
    ApplicationExitInfo.REASON_LOW_MEMORY -> "Killed for low memory"
    ApplicationExitInfo.REASON_EXCESSIVE_RESOURCE_USAGE -> "Killed for using too much"
    ApplicationExitInfo.REASON_INITIALIZATION_FAILURE -> "Failed to start"
    else -> "Exit $reason"
  }

  private fun header(context: Context, title: String): String {
    val info = context.packageManager.getPackageInfo(context.packageName, 0)
    val soc = if (Build.VERSION.SDK_INT >= 31) "${Build.SOC_MANUFACTURER} ${Build.SOC_MODEL}" else "unknown"
    val renderer = File(context.filesDir, "renderer").takeIf { it.isFile }?.readText()?.trim() ?: "default"
    return buildString {
      append("==== $title ====\n")
      append("app        ${context.packageName} ${info.versionName}\n")
      append("device     ${Build.MANUFACTURER} ${Build.MODEL}, soc $soc\n")
      append("android    ${Build.VERSION.RELEASE} (API ${Build.VERSION.SDK_INT}), ${Build.FINGERPRINT}\n")
      append("gpu        ${if (File("/dev/kgsl-3d0").exists()) "adreno" else if (File("/dev/mali0").exists()) "mali" else "other"}, renderer $renderer\n")
    }
  }

  private fun stamp(millis: Long): String = SimpleDateFormat("yyyy-MM-dd HH:mm:ss Z", Locale.ROOT).format(Date(millis))

  private fun write(context: Context, kind: String, text: String, tombstone: ByteArray?, at: Long = System.currentTimeMillis()) {
    val dir = folder(context)
    dir.mkdirs()
    val name = SimpleDateFormat("yyyy-MM-dd_HH-mm-ss", Locale.ROOT).format(Date(at)) + "_" + kind
    File(dir, "$name.txt").writeText(text)
    // Read with Android's pbtombstone, or any protobuf reader (tombstone.proto).
    tombstone?.let { File(dir, "$name.tombstone.pb").writeBytes(it) }
    Log.i(TAG, "wrote crash log $name")
  }

  /** Keeps the newest logs only. */
  private fun prune(dir: File) {
    val logs = dir.listFiles { f -> f.name.endsWith(".txt") }?.sortedByDescending { it.name } ?: return
    for (old in logs.drop(KEEP)) {
      old.delete()
      File(dir, old.name.removeSuffix(".txt") + ".tombstone.pb").delete()
    }
  }
}
