package br.edu.ifrn.chamados

import android.app.Activity
import android.os.Bundle
import android.util.Log
import android.widget.ScrollView
import android.widget.TextView
import kotlinx.coroutines.runBlocking
import uniffi.chamados_mobile.coreVersion
import uniffi.chamados_mobile.defaultListingPath
import uniffi.chamados_mobile.parseListing
import uniffi.chamados_mobile.probeSuap

class MainActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val out = StringBuilder()
        fun say(line: String) {
            out.appendLine(line)
            Log.i("CHAMADOS_SPIKE", line)
        }
        Thread {
            try {
                say("core_version=${coreVersion()}")
                say("default_listing_path=${defaultListingPath()}")
                val html = """<div class="general-box"><span class="status">Aberto</span>
                    <h4><a href="/centralservicos/chamado/7/">REQ #7 <strong>Assunto</strong></a></h4></div>"""
                val tickets = parseListing(html, "https://suap.example/")
                say("parse_listing=" + tickets.joinToString { "#${it.id} ${it.status} ${it.subject}" })
                val ok = runBlocking { probeSuap(filesDir.absolutePath, "http://10.0.2.2:8000/") }
                say("probe_suap(local, sem sessão)=$ok")
                val https = runCatching { runBlocking { probeSuap(filesDir.absolutePath, "https://suap.ifrn.edu.br/") } }
                say("probe_suap(https://suap.ifrn.edu.br)=${https.getOrElse { "ERRO: ${it.message}" }}")
                say("SPIKE_OK")
            } catch (e: Throwable) {
                say("SPIKE_ERRO: $e")
            }
            runOnUiThread {
                setContentView(ScrollView(this).apply { addView(TextView(this@MainActivity).apply { text = out.toString(); textSize = 14f }) })
            }
        }.start()
    }
}
