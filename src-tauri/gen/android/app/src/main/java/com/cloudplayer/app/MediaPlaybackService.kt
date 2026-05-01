package com.cloudplayer.app

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.os.Build
import android.os.IBinder
import android.support.v4.media.MediaMetadataCompat
import android.support.v4.media.session.MediaSessionCompat
import android.support.v4.media.session.PlaybackStateCompat
import androidx.core.app.NotificationCompat
import androidx.media.session.MediaButtonReceiver
import java.io.File
import java.net.HttpURLConnection
import java.net.URL

class MediaPlaybackService : Service() {

  private lateinit var mediaSession: MediaSessionCompat
  private var currentTitle: String = ""
  private var currentArtist: String = ""
  private var currentCoverUrl: String? = null
  private var currentDurationMs: Long = 0
  private var currentBitmap: Bitmap? = null

  override fun onCreate() {
    super.onCreate()
    createNotificationChannel()

    mediaSession = MediaSessionCompat(this, "CloudPlayerMedia").apply {
      setCallback(object : MediaSessionCompat.Callback() {
        override fun onPlay() { MainActivity.dispatchMediaCallback("play") }
        override fun onPause() { MainActivity.dispatchMediaCallback("pause") }
        override fun onSkipToNext() { MainActivity.dispatchMediaCallback("next") }
        override fun onSkipToPrevious() { MainActivity.dispatchMediaCallback("prev") }
        override fun onSeekTo(posMs: Long) { MainActivity.dispatchMediaCallback("seek:$posMs") }
        override fun onStop() {
          MainActivity.dispatchMediaCallback("stop")
          stopForeground(STOP_FOREGROUND_REMOVE)
          stopSelf()
        }
      })
      isActive = true
    }
  }

  override fun onBind(intent: Intent?): IBinder? = null

  override fun onDestroy() {
    mediaSession.isActive = false
    mediaSession.release()
    currentBitmap?.recycle()
    currentBitmap = null
    super.onDestroy()
  }

  private fun createNotificationChannel() {
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
      val channel = NotificationChannel(
        CHANNEL_ID,
        "媒体播放",
        NotificationManager.IMPORTANCE_LOW
      ).apply {
        description = "CloudPlayer 媒体播放控制"
        setShowBadge(false)
      }
      getSystemService(NotificationManager::class.java).createNotificationChannel(channel)
    }
  }

  private fun buildNotification(playing: Boolean): Notification {
    val launchIntent = packageManager.getLaunchIntentForPackage(packageName)?.apply {
      flags = Intent.FLAG_ACTIVITY_SINGLE_TOP
    }
    val pendingFlags = PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
    val contentPi = PendingIntent.getActivity(this, 0, launchIntent, pendingFlags)

    // 三键：prev / play_or_pause / next
    val prevPi = buildMediaActionPi("prev", android.R.drawable.ic_media_previous, 1)
    val playPauseAction = if (playing) {
      val pausePi = buildMediaActionPi("pause", android.R.drawable.ic_media_pause, 2)
      NotificationCompat.Action.Builder(android.R.drawable.ic_media_pause, "暂停", pausePi).build()
    } else {
      val playPi = buildMediaActionPi("play", android.R.drawable.ic_media_play, 2)
      NotificationCompat.Action.Builder(android.R.drawable.ic_media_play, "播放", playPi).build()
    }
    val nextPi = buildMediaActionPi("next", android.R.drawable.ic_media_next, 3)

    val builder = NotificationCompat.Builder(this, CHANNEL_ID)
      .setContentTitle(currentTitle)
      .setContentText(currentArtist)
      .setSubText("")
      .setSmallIcon(android.R.drawable.ic_media_play)
      .setContentIntent(contentPi)
      .setDeleteIntent(buildMediaActionPi("stop", 0, 4))
      .setVisibility(NotificationCompat.VISIBILITY_PUBLIC)
      .addAction(NotificationCompat.Action.Builder(android.R.drawable.ic_media_previous, "上一首", prevPi).build())
      .addAction(playPauseAction)
      .addAction(NotificationCompat.Action.Builder(android.R.drawable.ic_media_next, "下一首", nextPi).build())
      .setStyle(
        androidx.media.app.NotificationCompat.MediaStyle()
          .setMediaSession(mediaSession.sessionToken)
          .setShowActionsInCompactView(0, 1, 2)
      )
      .setOngoing(playing)
      .setShowWhen(false)

    currentBitmap?.let { builder.setLargeIcon(it) }

    return builder.build()
  }

  private fun buildMediaActionPi(action: String, icon: Int, requestCode: Int): PendingIntent {
    val intent = Intent(this, MediaPlaybackService::class.java).apply {
      this.action = "com.cloudplayer.app.MEDIA_ACTION.$action"
    }
    return PendingIntent.getService(this, requestCode, intent, PendingIntent.FLAG_IMMUTABLE)
  }

  private fun updateMetadata(title: String, artist: String, durationMs: Long) {
    val builder = MediaMetadataCompat.Builder()
      .putString(MediaMetadataCompat.METADATA_KEY_TITLE, title)
      .putString(MediaMetadataCompat.METADATA_KEY_ARTIST, artist)
    if (durationMs > 0) {
      builder.putLong(MediaMetadataCompat.METADATA_KEY_DURATION, durationMs)
    }
    currentBitmap?.let { builder.putBitmap(MediaMetadataCompat.METADATA_KEY_ALBUM_ART, it) }
    mediaSession.setMetadata(builder.build())
  }

  private fun downloadCover(urlStr: String): Bitmap? {
    return try {
      val cacheFile = File(cacheDir, "media_cover.jpg")
      // 简单缓存：如果 URL 未变就不重新下载
      val cachedUrl = try { File(cacheDir, "media_cover_url.txt").readText() } catch (_: Exception) { "" }
      if (cachedUrl == urlStr && cacheFile.exists()) {
        return BitmapFactory.decodeFile(cacheFile.absolutePath)
      }
      val url = URL(urlStr)
      val conn = url.openConnection() as HttpURLConnection
      conn.connectTimeout = 5000
      conn.readTimeout = 5000
      conn.doInput = true
      conn.connect()
      if (conn.responseCode == 200) {
        val bitmap = BitmapFactory.decodeStream(conn.inputStream)
        conn.inputStream.close()
        bitmap?.let {
          cacheFile.outputStream().use { fos -> it.compress(Bitmap.CompressFormat.JPEG, 85, fos) }
          File(cacheDir, "media_cover_url.txt").writeText(urlStr)
        }
        bitmap
      } else {
        conn.disconnect()
        null
      }
    } catch (_: Exception) {
      null
    }
  }

  companion object {
    private const val CHANNEL_ID = "cloudplayer_media"
    private const val NOTIFICATION_ID = 1001

    fun update(ctx: Context, title: String, artist: String, coverUrl: String?, durationMs: Long) {
      val intent = Intent(ctx, MediaPlaybackService::class.java)
      intent.putExtra("action", "update")
      intent.putExtra("title", title)
      intent.putExtra("artist", artist)
      intent.putExtra("coverUrl", coverUrl)
      intent.putExtra("durationMs", durationMs)
      if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
        ctx.startForegroundService(intent)
      } else {
        ctx.startService(intent)
      }
    }

    fun setPlayState(ctx: Context, playing: Boolean, positionMs: Long) {
      val intent = Intent(ctx, MediaPlaybackService::class.java)
      intent.putExtra("action", "setPlayState")
      intent.putExtra("playing", playing)
      intent.putExtra("positionMs", positionMs)
      if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
        ctx.startForegroundService(intent)
      } else {
        ctx.startService(intent)
      }
    }

    fun clear(ctx: Context) {
      ctx.stopService(Intent(ctx, MediaPlaybackService::class.java))
    }
  }

  private fun handleUpdate(intent: Intent) {
    currentTitle = intent.getStringExtra("title") ?: ""
    currentArtist = intent.getStringExtra("artist") ?: ""
    val newCoverUrl = intent.getStringExtra("coverUrl")
    currentDurationMs = intent.getLongExtra("durationMs", 0)

    // 封面变化时异步下载
    if (newCoverUrl != null && newCoverUrl != currentCoverUrl) {
      currentCoverUrl = newCoverUrl
      Thread {
        val bmp = downloadCover(newCoverUrl)
        if (bmp != null) {
          currentBitmap?.recycle()
          currentBitmap = bmp
          val scaled = Bitmap.createScaledBitmap(bmp, 256, 256, true)
          currentBitmap = scaled
          if (scaled !== bmp) bmp.recycle()
        }
        updateMetadata(currentTitle, currentArtist, currentDurationMs)
        showNotification(playing = true)
      }.start()
    } else {
      updateMetadata(currentTitle, currentArtist, currentDurationMs)
      showNotification(playing = true)
    }
  }

  private fun showNotification(playing: Boolean) {
    val notification = buildNotification(playing)
    startForeground(NOTIFICATION_ID, notification)
  }

  private fun handleSetPlayState(intent: Intent) {
    val playing = intent.getBooleanExtra("playing", false)
    val positionMs = intent.getLongExtra("positionMs", 0)

    val stateBuilder = PlaybackStateCompat.Builder()
      .setActions(
        PlaybackStateCompat.ACTION_PLAY or
        PlaybackStateCompat.ACTION_PAUSE or
        PlaybackStateCompat.ACTION_SKIP_TO_NEXT or
        PlaybackStateCompat.ACTION_SKIP_TO_PREVIOUS or
        PlaybackStateCompat.ACTION_SEEK_TO or
        PlaybackStateCompat.ACTION_STOP
      )
      .setState(
        if (playing) PlaybackStateCompat.STATE_PLAYING else PlaybackStateCompat.STATE_PAUSED,
        positionMs,
        1.0f
      )
    mediaSession.setPlaybackState(stateBuilder.build())
    showNotification(playing)
  }

  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    if (intent == null) {
      MediaButtonReceiver.handleIntent(mediaSession, null)
      return START_STICKY
    }

    // 媒体按钮转发
    if (intent.action?.startsWith("android.intent.action.MEDIA_BUTTON") == true) {
      MediaButtonReceiver.handleIntent(mediaSession, intent)
      return START_STICKY
    }

    // 自定义媒体操作
    val action = intent.getStringExtra("action")
    when (action) {
      "update" -> handleUpdate(intent)
      "setPlayState" -> handleSetPlayState(intent)
      else -> MediaButtonReceiver.handleIntent(mediaSession, intent)
    }

    return START_STICKY
  }
}
