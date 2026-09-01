package fr.eosuivi.elevage

import android.app.Activity
import android.os.Bundle
import android.view.View
import android.widget.ArrayAdapter
import androidx.appcompat.app.AppCompatActivity
import fr.eosuivi.elevage.databinding.ActivityConnexionBinding

/**
 * Écran de connexion : recherche des serveurs sur le réseau, ou saisie
 * manuelle d'une adresse.
 *
 * S'affiche au premier lancement, et à la demande depuis l'écran principal
 * (« Changer de serveur »).
 */
class ConnexionActivity : AppCompatActivity() {

    private lateinit var vue: ActivityConnexionBinding
    private lateinit var reglages: Reglages
    private lateinit var decouverte: Decouverte
    private val trouves = mutableListOf<Serveur>()
    private lateinit var adaptateur: ArrayAdapter<String>

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        vue = ActivityConnexionBinding.inflate(layoutInflater)
        setContentView(vue.root)
        reglages = Reglages(this)
        decouverte = Decouverte(this)

        adaptateur = ArrayAdapter(this, android.R.layout.simple_list_item_1, mutableListOf())
        vue.listeServeurs.adapter = adaptateur
        vue.listeServeurs.setOnItemClickListener { _, _, position, _ ->
            trouves.getOrNull(position)?.let { enregistrer(it.url, externe = null) }
        }

        vue.champInterne.setText(reglages.adresseInterne.orEmpty())
        vue.champExterne.setText(reglages.adresseExterne.orEmpty())

        vue.boutonRechercher.setOnClickListener { rechercher() }
        vue.boutonValider.setOnClickListener { validerSaisie() }

        rechercher()
    }

    private fun rechercher() {
        trouves.clear()
        adaptateur.clear()
        vue.etatRecherche.text = getString(R.string.recherche_en_cours)
        vue.chargement.visibility = View.VISIBLE
        decouverte.demarrer(
            surTrouve = { serveur ->
                runOnUiThread {
                    if (trouves.none { it.hote == serveur.hote && it.port == serveur.port }) {
                        trouves += serveur
                        adaptateur.add("${serveur.nom}\n${serveur.hote}:${serveur.port}")
                        vue.etatRecherche.text =
                            resources.getQuantityString(R.plurals.serveurs_trouves, trouves.size, trouves.size)
                        vue.chargement.visibility = View.GONE
                    }
                }
            },
            surErreur = { message ->
                runOnUiThread {
                    vue.chargement.visibility = View.GONE
                    vue.etatRecherche.text = message
                }
            },
        )
        // La recherche reste active tant que l'écran est ouvert ; au bout de
        // quelques secondes sans résultat, on le dit plutôt que de laisser
        // tourner un indicateur sans fin.
        vue.root.postDelayed({
            if (trouves.isEmpty() && !isFinishing) {
                vue.chargement.visibility = View.GONE
                vue.etatRecherche.setText(R.string.aucun_serveur)
            }
        }, 6000)
    }

    private fun validerSaisie() {
        val interne = Adresse.normaliser(vue.champInterne.text.toString())
        val externeSaisie = vue.champExterne.text.toString()
        val externe = if (externeSaisie.isBlank()) null else Adresse.normaliser(externeSaisie)

        if (interne == null && externe == null) {
            vue.champInterne.error = getString(R.string.adresse_invalide)
            return
        }
        if (externeSaisie.isNotBlank() && externe == null) {
            vue.champExterne.error = getString(R.string.adresse_invalide)
            return
        }
        enregistrer(interne, externe)
    }

    private fun enregistrer(interne: String?, externe: String?) {
        reglages.adresseInterne = interne
        reglages.adresseExterne = externe
        setResult(Activity.RESULT_OK)
        finish()
    }

    override fun onDestroy() {
        decouverte.arreter()
        super.onDestroy()
    }
}
