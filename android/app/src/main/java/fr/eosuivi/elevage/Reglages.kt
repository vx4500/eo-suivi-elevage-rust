package fr.eosuivi.elevage

import android.content.Context

/**
 * Les deux adresses du serveur, mémorisées entre deux lancements.
 *
 * Même principe que l'application Home Assistant : une adresse interne (le
 * réseau de l'élevage) et une adresse externe facultative (nom de domaine
 * public). Un élevage sans internet ne renseigne que l'interne.
 */
class Reglages(context: Context) {
    private val prefs = context.getSharedPreferences("eo-suivi", Context.MODE_PRIVATE)

    var adresseInterne: String?
        get() = prefs.getString(CLE_INTERNE, null)
        set(valeur) = prefs.edit().putString(CLE_INTERNE, valeur).apply()

    var adresseExterne: String?
        get() = prefs.getString(CLE_EXTERNE, null)
        set(valeur) = prefs.edit().putString(CLE_EXTERNE, valeur).apply()

    /** Vrai tant qu'aucune adresse n'est connue : l'écran de connexion s'impose. */
    fun vierge(): Boolean = adresseInterne.isNullOrBlank() && adresseExterne.isNullOrBlank()

    fun oublier() {
        prefs.edit().remove(CLE_INTERNE).remove(CLE_EXTERNE).apply()
    }

    private companion object {
        const val CLE_INTERNE = "adresse_interne"
        const val CLE_EXTERNE = "adresse_externe"
    }
}
