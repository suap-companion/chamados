package br.edu.ifrn.chamados

import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.FilterChip
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import java.text.DateFormat
import java.util.Date
import uniffi.chamados_mobile.PlaceItem
import uniffi.chamados_mobile.ServiceItem

/** Os passos de "Novo chamado". */
enum class WizardStep(val label: String) { Service("Serviço"), Interested("Interessado"), Description("Descrição") }

/** "Passo 1 de 3 · Serviço". */
fun stepTitle(step: WizardStep): String = "Passo ${step.ordinal + 1} de ${WizardStep.entries.size} · ${step.label}"

/** A fração da barra de progresso: o passo atual conta como feito. */
fun stepProgress(step: WizardStep): Float = (step.ordinal + 1).toFloat() / WizardStep.entries.size

/** Quantos serviços usados por último se lembram. */
const val MAX_RECENTS = 5

/** Põe `id` no começo dos recentes, sem repetir, até [MAX_RECENTS]. */
fun withRecent(recents: List<String>, id: String): List<String> = (listOf(id) + recents.filter { it != id }).take(MAX_RECENTS)

/** Os recentes guardados como "400,12,7" (ignora o que não for número). */
fun parseRecents(text: String): List<String> = text.split(',').map { it.trim() }.filter { it.isNotEmpty() && it.all(Char::isDigit) }

/** O que vem escolhido numa lista de campus ou centros: o preferido do SUAP, ou o primeiro. */
fun defaultPlace(places: List<PlaceItem>): PlaceItem? = places.firstOrNull { it.preferred } ?: places.firstOrNull()

/**
 * Com mais de um centro de atendimento a escolha é obrigatória (como no `chamados open`); com um só, ele
 * vale sozinho e com nenhum não há como abrir.
 */
fun centerRequired(centers: List<PlaceItem>): Boolean = centers.size > 1

/** O texto do número do serviço é um número válido (o SUAP usa inteiros positivos). */
fun validServiceId(text: String): Boolean = text.trim().toLongOrNull()?.let { it > 0 } == true

/** Pode ir ao passo seguinte? Serviço: um número válido e o centro escolhido quando há vários. */
fun canContinue(step: WizardStep, serviceId: String, centers: List<PlaceItem>, center: String?, description: String): Boolean =
    when (step) {
        WizardStep.Service -> validServiceId(serviceId) && (!centerRequired(centers) || center != null)
        WizardStep.Interested -> true
        WizardStep.Description -> description.isNotBlank()
    }

/** Quando o catálogo foi lido, para a pessoa saber se está velho. */
fun catalogLine(info: uniffi.chamados_mobile.CatalogInfo?): String {
    val read = info?.fetchedAt
    return when {
        info == null || read == null -> "Catálogo ainda não lido. Toque em Atualizar catálogo."
        else -> "${info.count} serviços · lido em ${DateFormat.getDateTimeInstance(DateFormat.SHORT, DateFormat.SHORT).format(Date(read.toLong()))}"
    }
}

/** O assistente de "Novo chamado": Serviço, Interessado e Descrição, com barra de progresso e Continuar no rodapé. */
@Composable
fun NewTicketScreen(model: MainViewModel) {
    BackHandler { model.wizardBack() }
    val step = model.wizardStep
    val ready = canContinue(step, model.serviceId, model.centers, model.center, model.draft)
    Scaffold(
        topBar = { GovBrHeader("Novo chamado", onBack = model::wizardBack) },
        bottomBar = {
            Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                model.actionMessage?.let { Text(it, color = if (model.actionIsError) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.primary) }
                if (step == WizardStep.Description) {
                    Button(onClick = { model.createTicket(model.draft) }, enabled = ready && !model.actionBusy, modifier = Modifier.fillMaxWidth()) {
                        Text(if (model.actionBusy) "Enviando…" else "Abrir chamado")
                    }
                } else {
                    Button(onClick = model::wizardNext, enabled = ready, modifier = Modifier.fillMaxWidth()) { Text("Continuar") }
                }
            }
        },
    ) { padding ->
        Column(Modifier.padding(padding).fillMaxSize()) {
            Column(Modifier.padding(horizontal = 16.dp, vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(stepTitle(step), style = MaterialTheme.typography.labelLarge)
                LinearProgressIndicator(progress = { stepProgress(step) }, modifier = Modifier.fillMaxWidth())
            }
            Column(Modifier.padding(horizontal = 16.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                when (step) {
                    WizardStep.Service -> ServiceStep(model)
                    WizardStep.Interested -> InterestedStep(model)
                    WizardStep.Description -> DescriptionStep(model)
                }
            }
        }
    }
}

/** Uma linha de serviço escolhível: nome, e embaixo o grupo e o número; marca de incidente e de interno. */
@Composable
fun ServiceRow(name: String, detail: String, selected: Boolean, onClick: () -> Unit) {
    Card(
        modifier = Modifier.fillMaxWidth().clickable(onClick = onClick),
        colors = CardDefaults.cardColors(
            containerColor = if (selected) MaterialTheme.colorScheme.primaryContainer else MaterialTheme.colorScheme.surfaceVariant,
        ),
    ) {
        Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(name, style = MaterialTheme.typography.titleSmall)
            Text(detail, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
}

private fun ServiceItem.detail(): String =
    listOfNotNull(group.ifBlank { null }, "#$id", if (incident) "incidente" else null, if (internal) "interno" else null).joinToString(" · ")

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun ServiceStep(model: MainViewModel) {
    LaunchedEffect(Unit) { model.loadCatalogInfo() }
    val defaultId = model.profileServiceId
    if (defaultId.isNotBlank()) {
        Text("Padrão do perfil", style = MaterialTheme.typography.titleSmall)
        val known = model.serviceName(defaultId)
        ServiceRow(known ?: "Serviço #$defaultId", "#$defaultId", model.serviceId.trim() == defaultId) { model.chooseService(defaultId) }
    }
    if (model.recents.isNotEmpty()) {
        Text("Recentes", style = MaterialTheme.typography.titleSmall)
        model.recents.filter { it != defaultId }.forEach { id ->
            ServiceRow(model.serviceName(id) ?: "Serviço #$id", "#$id", model.serviceId.trim() == id) { model.chooseService(id) }
        }
    }
    Text("Buscar serviço", style = MaterialTheme.typography.titleSmall)
    OutlinedTextField(
        value = model.serviceQuery,
        onValueChange = model::searchServices,
        label = { Text("Nome ou número do serviço") },
        singleLine = true,
        modifier = Modifier.fillMaxWidth(),
    )
    model.serviceResults.forEach { item ->
        ServiceRow(item.name, item.detail(), model.serviceId.trim() == item.id.toString()) { model.chooseService(item.id.toString(), item.name) }
    }
    if (model.serviceQuery.isNotBlank() && model.serviceResults.isEmpty() && validServiceId(model.serviceQuery)) {
        OutlinedButton(onClick = { model.chooseService(model.serviceQuery.trim()) }, modifier = Modifier.fillMaxWidth()) {
            Text("Usar o serviço #${model.serviceQuery.trim()}")
        }
    }
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(catalogLine(model.catalogInfo), style = MaterialTheme.typography.labelMedium, modifier = Modifier.weight(1f))
        TextButton(onClick = model::refreshCatalog, enabled = !model.catalogBusy) { Text(if (model.catalogBusy) "Lendo…" else "Atualizar catálogo") }
    }
    model.catalogMessage?.let { Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.labelMedium) }
    if (model.campuses.isNotEmpty()) {
        Text("Campus", style = MaterialTheme.typography.titleSmall)
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            model.campuses.forEach { place ->
                FilterChip(selected = model.campus == place.id, onClick = { model.chooseCampus(place.id) }, label = { Text(place.name) })
            }
        }
    }
    if (model.centers.isNotEmpty()) {
        Text(if (centerRequired(model.centers)) "Centro de atendimento (escolha um)" else "Centro de atendimento", style = MaterialTheme.typography.titleSmall)
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            model.centers.forEach { place ->
                FilterChip(selected = model.center == place.id, onClick = { model.chooseCenter(place.id) }, label = { Text(place.name) })
            }
        }
    }
    model.placesMessage?.let { Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.labelMedium) }
}

@Composable
private fun InterestedStep(model: MainViewModel) {
    OutlinedTextField(
        value = model.interested,
        onValueChange = { model.interested = it },
        label = { Text("Interessado (vínculo; deixe vazio se o serviço não pedir)") },
        singleLine = true,
        modifier = Modifier.fillMaxWidth(),
    )
}

@Composable
private fun DescriptionStep(model: MainViewModel) {
    var text by remember { mutableStateOf(model.draft) }
    val picker = rememberLauncherForActivityResult(ActivityResultContracts.GetMultipleContents()) { uris ->
        if (uris.isNotEmpty()) model.onPicked(uris, 3u)
    }
    OutlinedTextField(
        value = text,
        onValueChange = { text = it; model.draft = it },
        label = { Text("Descrição") },
        minLines = 5,
        modifier = Modifier.fillMaxWidth(),
    )
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        OutlinedButton(onClick = { picker.launch("*/*") }) { Text("Anexar (até 3)") }
        if (model.picked.isNotEmpty()) TextButton(onClick = model::clearPicked) { Text("Limpar") }
    }
    model.picked.forEach { Text("• ${it.name}") }
    model.pickNote?.let { Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.labelMedium) }
}
