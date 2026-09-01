package fr.eosuivi.elevage

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.net.InetSocketAddress
import java.net.Socket
import java.net.URI

/**
 * Choix de l'adresse à ouvrir au lancement.
 *
 * L'application Home Assistant bascule interne/externe selon le SSID Wi-Fi.
 * On teste plutôt la joignabilité réelle : lire le SSID exige depuis Android
 * 10 la permission de localisation précise, que personne ne comprend quand on
 * la demande pour « trouver un serveur », et un même élevage peut avoir
 * plusieurs bornes. Un essai de connexion TCP d'une seconde répond à la vraie
 * question — « est-ce que le serveur local répond, ici, maintenant ? » — sans
 * aucune permission.
 */
object Joignable {

    /**
     * Renvoie l'adresse interne si son serveur répond, sinon l'externe, sinon
     * ce qui est disponible. `null` si rien n'est configuré.
     */
    suspend fun choisir(interne: String?, externe: String?): String? {
        val interneUtilisable = interne?.takeIf { it.isNotBlank() }
        val externeUtilisable = externe?.takeIf { it.isNotBlank() }
        if (interneUtilisable == null) return externeUtilisable
        if (externeUtilisable == null) return interneUtilisable
        return if (repond(interneUtilisable)) interneUtilisable else externeUtilisable
    }

    /** Essai de connexion TCP, borné à [delaiMs]. Aucune requête HTTP émise. */
    suspend fun repond(url: String, delaiMs: Int = 1000): Boolean = withContext(Dispatchers.IO) {
        val uri = runCatching { URI(url) }.getOrNull() ?: return@withContext false
        val hote = uri.host ?: return@withContext false
        val port = if (uri.port > 0) uri.port else if (uri.scheme == "https") 443 else 80
        runCatching {
            Socket().use { socket ->
                socket.connect(InetSocketAddress(hote, port), delaiMs)
                true
            }
        }.getOrDefault(false)
    }
}
