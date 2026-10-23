package br.edu.ifrn.chamados

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Home
import androidx.compose.material.icons.filled.Menu
import androidx.compose.material.icons.filled.Person
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FloatingActionButton
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import java.time.LocalDate
import java.time.format.TextStyle
import java.util.Locale
import uniffi.chamados_mobile.Queue
import uniffi.chamados_mobile.SnoozeItem
import uniffi.chamados_mobile.TicketSummary

/** Os três números do topo da tela Hoje. */
data class TodayCounters(val attending: Int, val overdue: Int, val returned: Int)

/**
 * O que a tela Hoje mostra: os contadores e as seções (voltaram do adiamento, em aberto, resolvidos), mais os
 * chamados com o SLA estourado (para a marca vermelha nos cartões).
 */
data class TodayData(
    val counters: TodayCounters,
    val returned: List<TicketSummary>,
    val open: List<TicketSummary>,
    val resolved: List<TicketSummary>,
    val overdueIds: Set<String>,
)

private val FINISHED = listOf("resolvido", "fechado", "cancelado")

private fun TicketSummary.statusText(): String = status.orEmpty().trim().lowercase(Locale.ROOT)

/** O chamado está sendo atendido agora ("Em atendimento"). */
fun TicketSummary.isAttending(): Boolean = statusText().contains("atendimento")

/** O chamado já saiu da fila de trabalho (resolvido, fechado ou cancelado). */
fun TicketSummary.isFinished(): Boolean = statusText().let { text -> FINISHED.any { text.startsWith(it) } }

/**
 * Monta a tela Hoje a partir das listas que o app já busca: os meus ativos, os adiados (os que voltaram são os
 * `due` em `snoozes`), os meus com SLA estourado e os meus resolvidos. Um chamado aparece numa seção só
 * (voltou, depois aberto, depois resolvido) e, entre os abertos, os de SLA estourado vêm primeiro.
 */
fun todayData(
    mine: List<TicketSummary>,
    snoozed: List<TicketSummary>,
    overdue: List<TicketSummary>,
    resolved: List<TicketSummary>,
    snoozes: Map<String, SnoozeItem>,
): TodayData {
    val overdueIds = overdue.map { it.id }.toSet()
    val returned = snoozed.filter { snoozes[it.id]?.due == true }.distinctBy { it.id }
    val returnedIds = returned.map { it.id }.toSet()
    val active = mine.filter { !it.isFinished() && it.id !in returnedIds }.distinctBy { it.id }
    val open = active.filter { it.id in overdueIds } + active.filter { it.id !in overdueIds }
    val shown = returnedIds + open.map { it.id }
    val done = (resolved + mine.filter { it.isFinished() }).distinctBy { it.id }.filter { it.id !in shown }
    return TodayData(
        counters = TodayCounters(
            attending = (open + returned).count { it.isAttending() },
            overdue = overdueIds.size,
            returned = returned.size,
        ),
        returned = returned,
        open = open,
        resolved = done,
        overdueIds = overdueIds,
    )
}

/** A data por extenso, em português do Brasil: "sexta-feira, 23 de outubro de 2026". */
fun longDate(date: LocalDate): String {
    val locale = Locale.forLanguageTag("pt-BR")
    val week = date.dayOfWeek.getDisplayName(TextStyle.FULL, locale)
    val month = date.month.getDisplayName(TextStyle.FULL, locale)
    return "$week, ${date.dayOfMonth} de $month de ${date.year}"
}

/** Os lugares da navegação inferior. */
enum class HomeTab(val label: String) { Today("Hoje"), Mine("Meus"), Support("Suporte"), ToClose("A fechar") }

fun HomeTab.queue(): Queue? = when (this) {
    HomeTab.Today -> null
    HomeTab.Mine -> Queue.MINE
    HomeTab.Support -> Queue.SUPPORT
    HomeTab.ToClose -> Queue.TO_CLOSE
}

/** A barra inferior: Hoje e as três listas. */
@Composable
fun HomeNavigation(selected: HomeTab, onSelect: (HomeTab) -> Unit) {
    NavigationBar {
        HomeTab.entries.forEach { tab ->
            NavigationBarItem(
                selected = tab == selected,
                onClick = { onSelect(tab) },
                icon = {
                    Icon(
                        when (tab) {
                            HomeTab.Today -> Icons.Filled.Home
                            HomeTab.Mine -> Icons.Filled.Person
                            HomeTab.Support -> Icons.Filled.Menu
                            HomeTab.ToClose -> Icons.Filled.Check
                        },
                        contentDescription = null,
                    )
                },
                label = { Text(tab.label, maxLines = 1) },
            )
        }
    }
}

/** Um contador do topo: o número grande e o rótulo; tocar abre a lista filtrada. */
@Composable
fun CounterCard(label: String, value: Int, warning: Boolean, modifier: Modifier = Modifier, onClick: () -> Unit) {
    val color = if (warning && value > 0) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurface
    Card(
        modifier = modifier.clickable(role = Role.Button, onClick = onClick).semantics(mergeDescendants = true) { },
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceVariant),
    ) {
        Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(value.toString(), style = MaterialTheme.typography.headlineMedium, fontWeight = FontWeight.Bold, color = color)
            Text(label, style = MaterialTheme.typography.labelMedium, maxLines = 2)
        }
    }
}

/** O topo da tela Hoje: a data e os três contadores. Sem estado. */
@Composable
fun TodayHeader(date: LocalDate, counters: TodayCounters, onAttending: () -> Unit, onOverdue: () -> Unit, onReturned: () -> Unit) {
    Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Text(longDate(date).replaceFirstChar { it.titlecase(Locale.forLanguageTag("pt-BR")) }, style = MaterialTheme.typography.titleLarge)
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
            CounterCard("Em atendimento", counters.attending, warning = false, modifier = Modifier.weight(1f), onClick = onAttending)
            CounterCard("SLA estourado", counters.overdue, warning = true, modifier = Modifier.weight(1f), onClick = onOverdue)
            CounterCard("Voltou do adiamento", counters.returned, warning = false, modifier = Modifier.weight(1f), onClick = onReturned)
        }
    }
}

/** O título de uma seção, com a contagem. */
@Composable
fun SectionTitle(text: String, count: Int) {
    Text(
        "$text ($count)",
        style = MaterialTheme.typography.titleMedium,
        modifier = Modifier.fillMaxWidth().padding(start = 16.dp, end = 16.dp, top = 16.dp, bottom = 4.dp),
    )
}

/** A tela Hoje: a data, os contadores e as seções Voltou hoje, Em aberto e Resolvidos. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun TodayScreen(model: MainViewModel) {
    val data = model.today
    var newProfile by remember { mutableStateOf(false) }
    Scaffold(
        topBar = { TicketsHeader(model, searchable = false) { newProfile = true } },
        bottomBar = { HomeNavigation(HomeTab.Today, model::chooseTab) },
        floatingActionButton = {
            FloatingActionButton(onClick = model::openNewTicket) { Icon(Icons.Filled.Add, contentDescription = "Novo chamado") }
        },
    ) { padding ->
        PullToRefreshBox(isRefreshing = model.todayLoading, onRefresh = model::refreshToday, modifier = Modifier.padding(padding).fillMaxSize()) {
            LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(bottom = 88.dp)) {
                val error = model.todayError
                when {
                    error != null -> item { ListState(error, isError = true, onRetry = model::refreshToday) }
                    data == null -> item { TicketListSkeleton() }
                    else -> {
                        item {
                            TodayHeader(
                                date = LocalDate.now(),
                                counters = data.counters,
                                onAttending = { model.openList(Queue.MINE) },
                                onOverdue = { model.openList(Queue.SUPPORT, assignedToMe = true, slaOnly = true) },
                                onReturned = { model.openList(Queue.MINE, ListMode.Snoozed) },
                            )
                        }
                        section("Voltou hoje", data.returned, model, data)
                        section("Em aberto", data.open, model, data)
                        section("Resolvidos", data.resolved, model, data)
                        if (data.returned.isEmpty() && data.open.isEmpty() && data.resolved.isEmpty()) {
                            item { ListState("Nada por aqui hoje. Puxe para baixo para atualizar.", isError = false) }
                        }
                    }
                }
            }
        }
    }
    if (newProfile) NewProfileDialog(model) { newProfile = false }
}

private fun androidx.compose.foundation.lazy.LazyListScope.section(
    title: String,
    tickets: List<TicketSummary>,
    model: MainViewModel,
    data: TodayData,
) {
    if (tickets.isEmpty()) return
    item(key = "titulo-$title") { SectionTitle(title, tickets.size) }
    items(tickets, key = { "$title-${it.id}" }) { ticket ->
        TicketItem(
            ticket, model.titles[ticket.id], model.snoozes[ticket.id],
            photo = ticket.photo?.let { model.photoFiles[it] },
            overdue = ticket.id in data.overdueIds,
        ) { model.openDetail(ticket.id) }
        ListDivider()
    }
}
