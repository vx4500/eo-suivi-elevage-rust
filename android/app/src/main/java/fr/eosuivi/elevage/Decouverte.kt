package fr.eosuivi.elevage

import android.content.Context
import android.net.nsd.NsdManager
import android.net.nsd.NsdServiceInfo
import android.net.wifi.WifiManager
import android.os.Build
import android.util.Log

/** Un serveur EO-Suivi trouvé sur le réseau local. */
data class Serveur(val nom: String, val hote: String, val port: Int) {
    val url: String get() = Adresse.depuisDecouverte(hote, port)
}

/**
 * Découverte des serveurs EO-Suivi sur le réseau local (mDNS/DNS-SD).
 *
 * Le serveur Rust publie `_eosuivi._tcp` (module `src/mdns.rs`). Beaucoup
 * d'élevages n'ont ni domaine ni DNS : c'est le seul moyen de trouver le
 * serveur sans faire saisir une adresse IP à l'éleveur.
 *
 * La découverte n'est jamais garantie — des points d'accès Wi-Fi filtrent le
 * multicast, et le mDNS ne traverse pas les VLAN. L'écran de connexion garde
 * donc toujours la saisie manuelle.
 */
class Decouverte(private val context: Context) {

    private val nsd = context.getSystemService(Context.NSD_SERVICE) as NsdManager
    private var listener: NsdManager.DiscoveryListener? = null
    private var verrouMulticast: WifiManager.MulticastLock? = null

    /**
     * Démarre la recherche. [surTrouve] est appelé une fois par serveur
     * résolu, sur un thread de service — l'appelant remonte sur le thread UI.
     */
    fun demarrer(surTrouve: (Serveur) -> Unit, surErreur: (String) -> Unit) {
        arreter()
        // Sans ce verrou, Android peut jeter les paquets multicast en veille
        // Wi-Fi et la découverte ne trouve rien, sans erreur visible.
        verrouMulticast = (context.applicationContext.getSystemService(Context.WIFI_SERVICE) as? WifiManager)
            ?.createMulticastLock("eo-suivi-mdns")
            ?.apply {
                setReferenceCounted(true)
                runCatching { acquire() }
            }

        val ecouteur = object : NsdManager.DiscoveryListener {
            override fun onDiscoveryStarted(type: String) = Unit

            override fun onServiceFound(service: NsdServiceInfo) {
                resoudre(service, surTrouve)
            }

            override fun onServiceLost(service: NsdServiceInfo) = Unit
            override fun onDiscoveryStopped(type: String) = Unit

            override fun onStartDiscoveryFailed(type: String, code: Int) {
                surErreur("Recherche impossible sur ce réseau (code $code)")
            }

            override fun onStopDiscoveryFailed(type: String, code: Int) = Unit
        }
        listener = ecouteur
        runCatching { nsd.discoverServices(TYPE, NsdManager.PROTOCOL_DNS_SD, ecouteur) }
            .onFailure { surErreur("Recherche impossible : ${it.message}") }
    }

    /**
     * Android ne permet qu'une résolution à la fois par `NsdManager` sur les
     * versions anciennes : chaque service reçoit son propre écouteur, et une
     * collision (`FAILURE_ALREADY_ACTIVE`) est réessayée une fois plutôt que
     * de perdre silencieusement le serveur.
     */
    private fun resoudre(service: NsdServiceInfo, surTrouve: (Serveur) -> Unit, essai: Int = 0) {
        val ecouteur = object : NsdManager.ResolveListener {
            override fun onResolveFailed(info: NsdServiceInfo, code: Int) {
                if (code == NsdManager.FAILURE_ALREADY_ACTIVE && essai < 3) {
                    Thread.sleep(250L * (essai + 1))
                    resoudre(service, surTrouve, essai + 1)
                } else {
                    Log.w(TAG, "Résolution échouée pour ${info.serviceName} (code $code)")
                }
            }

            override fun onServiceResolved(info: NsdServiceInfo) {
                val hote = info.host?.hostAddress ?: return
                surTrouve(Serveur(nom = info.serviceName ?: "EO-Suivi", hote = hote, port = info.port))
            }
        }
        runCatching {
            @Suppress("DEPRECATION")
            nsd.resolveService(service, ecouteur)
        }
    }

    fun arreter() {
        listener?.let { runCatching { nsd.stopServiceDiscovery(it) } }
        listener = null
        verrouMulticast?.let { verrou -> runCatching { if (verrou.isHeld) verrou.release() } }
        verrouMulticast = null
    }

    private companion object {
        const val TYPE = "_eosuivi._tcp"
        const val TAG = "EOSuiviDecouverte"
        // Référencé pour éviter un avertissement « champ inutilisé » si la
        // version d'Android change la façon de résoudre les services.
        val VERSION_ANDROID = Build.VERSION.SDK_INT
    }
}
