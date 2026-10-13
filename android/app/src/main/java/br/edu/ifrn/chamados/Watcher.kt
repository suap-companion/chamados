package br.edu.ifrn.chamados

import android.Manifest
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.ExistingWorkPolicy
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import java.util.concurrent.TimeUnit
import uniffi.chamados_mobile.Queue
import uniffi.chamados_mobile.WatchItem

/** O que a pessoa escolheu para o acompanhamento em segundo plano (fica só no aparelho). */
data class WatchSettings(
    val enabled: Boolean = false,
    val minutes: Int = 60,
    val queue: Queue = Queue.MINE,
    val messages: Boolean = false,
)

object Watcher {
    private const val PREFS = "chamados"
    private const val PERIODIC = "acompanhar"
    private const val NOW = "acompanhar-agora"
    private const val CHANNEL = "acompanhamento"
    private const val SUMMARY_ID = 1
    private const val EXPIRED_ID = 2
    private const val MAX_SEPARATE = 5

    /** Intervalos oferecidos, em minutos; o Android não deixa menos de 15. */
    val intervals = listOf(15, 30, 60, 120)

    fun load(context: Context): WatchSettings {
        val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        val queue = if (prefs.getString("watch_queue", "mine") == "support") Queue.SUPPORT else Queue.MINE
        return WatchSettings(
            enabled = prefs.getBoolean("watch_enabled", false),
            minutes = prefs.getInt("watch_minutes", 60),
            queue = queue,
            messages = prefs.getBoolean("watch_messages", false),
        )
    }

    /** Guarda a escolha e liga ou desliga o agendamento. */
    fun save(context: Context, settings: WatchSettings) {
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit()
            .putBoolean("watch_enabled", settings.enabled)
            .putInt("watch_minutes", settings.minutes)
            .putString("watch_queue", if (settings.queue == Queue.SUPPORT) "support" else "mine")
            .putBoolean("watch_messages", settings.messages)
            .apply()
        schedule(context, settings)
    }

    private fun schedule(context: Context, settings: WatchSettings) {
        val work = WorkManager.getInstance(context)
        if (!settings.enabled) {
            work.cancelUniqueWork(PERIODIC)
            return
        }
        val request = PeriodicWorkRequestBuilder<WatchWorker>(settings.minutes.toLong(), TimeUnit.MINUTES)
            .setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build())
            .build()
        // UPDATE: mudar o intervalo vale já, sem perder o estado do que foi visto.
        work.enqueueUniquePeriodicWork(PERIODIC, ExistingPeriodicWorkPolicy.UPDATE, request)
    }

    /** Pede uma rodada agora (o resultado chega como notificação). */
    fun checkNow(context: Context) {
        val request = OneTimeWorkRequestBuilder<WatchWorker>()
            .setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build())
            .build()
        WorkManager.getInstance(context).enqueueUniqueWork(NOW, ExistingWorkPolicy.REPLACE, request)
    }

    fun canNotify(context: Context): Boolean =
        Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU ||
            ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED

    private fun ensureChannel(context: Context) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        val channel = NotificationChannel(CHANNEL, "Acompanhamento de chamados", NotificationManager.IMPORTANCE_DEFAULT)
        context.getSystemService(NotificationManager::class.java).createNotificationChannel(channel)
    }

    private fun base(context: Context): NotificationCompat.Builder {
        ensureChannel(context)
        val open = PendingIntent.getActivity(
            context, 0, Intent(context, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        // Na tela bloqueada o conteúdo do chamado não aparece: só um aviso genérico.
        val publicVersion = NotificationCompat.Builder(context, CHANNEL)
            .setSmallIcon(android.R.drawable.ic_dialog_info)
            .setContentTitle("Chamados")
            .setContentText("Há novidades nos seus chamados.")
            .build()
        return NotificationCompat.Builder(context, CHANNEL)
            .setSmallIcon(android.R.drawable.ic_dialog_info)
            .setContentIntent(open)
            .setAutoCancel(true)
            .setVisibility(NotificationCompat.VISIBILITY_PRIVATE)
            .setPublicVersion(publicVersion)
    }

    fun line(item: WatchItem): String {
        val what = when (item.kind) {
            "new" -> "Novo: ${item.to ?: "-"}"
            "status" -> "Situação: ${item.from ?: "-"} → ${item.to ?: "-"}"
            "left" -> "Saiu da lista (última situação: ${item.from ?: "-"})"
            else -> "Nova mensagem: ${item.text.orEmpty().take(120)}"
        }
        val name = item.title ?: item.subject
        return "#${item.id}${name?.let { " ($it)" }.orEmpty()}  $what"
    }

    /** Uma notificação por mudança (até 5); acima disso, um resumo só. */
    @Suppress("MissingPermission")
    fun notify(context: Context, items: List<WatchItem>) {
        if (items.isEmpty() || !canNotify(context)) return
        val manager = NotificationManagerCompat.from(context)
        if (items.size > MAX_SEPARATE) {
            val style = NotificationCompat.InboxStyle().also { box -> items.take(8).forEach { box.addLine(line(it)) } }
            val summary = base(context).setContentTitle("${items.size} novidades nos chamados")
                .setContentText(line(items.first())).setStyle(style).build()
            manager.notify(SUMMARY_ID, summary)
            return
        }
        items.forEach { item ->
            val notification = base(context).setContentTitle("Chamado #${item.id}").setContentText(line(item))
                .setStyle(NotificationCompat.BigTextStyle().bigText(line(item))).build()
            manager.notify(("${item.id}:${item.kind}").hashCode(), notification)
        }
    }

    /** Avisa (uma vez) que a sessão acabou e é preciso entrar de novo; `resetExpired` rearma o aviso. */
    @Suppress("MissingPermission")
    fun notifyExpired(context: Context) {
        val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        if (prefs.getBoolean("expired_notified", false) || !canNotify(context)) return
        prefs.edit().putBoolean("expired_notified", true).apply()
        val notification = base(context).setContentTitle("Chamados: sessão expirada")
            .setContentText("Abra o app e entre de novo para continuar acompanhando.").build()
        NotificationManagerCompat.from(context).notify(EXPIRED_ID, notification)
    }

    fun resetExpired(context: Context) {
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().putBoolean("expired_notified", false).apply()
    }
}

/** Uma rodada do acompanhamento: só lê o SUAP e avisa o que mudou. */
class WatchWorker(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {
    override suspend fun doWork(): Result {
        val settings = Watcher.load(applicationContext)
        return try {
            val core = CoreAccess.open(applicationContext)
            if (!core.isAuthenticated()) {
                Watcher.notifyExpired(applicationContext)
                return Result.success()
            }
            Watcher.resetExpired(applicationContext)
            Watcher.notify(applicationContext, core.watchOnce(settings.queue, false, settings.messages))
            Result.success()
        } catch (_: Throwable) {
            // Sem rede ou SUAP fora do ar: tenta de novo na próxima rodada (sem log de conteúdo).
            Result.success()
        }
    }
}
