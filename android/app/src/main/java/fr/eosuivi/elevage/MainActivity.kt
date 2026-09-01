package fr.eosuivi.elevage

import android.app.DownloadManager
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.os.Environment
import android.view.Menu
import android.view.MenuItem
import android.view.View
import android.webkit.CookieManager
import android.webkit.DownloadListener
import android.webkit.ValueCallback
import android.webkit.WebChromeClient
import android.webkit.WebResourceError
import android.webkit.WebResourceRequest
import android.webkit.WebSettings
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.activity.OnBackPressedCallback
import androidx.activity.result.contract.ActivityResultContracts
import androidx.appcompat.app.AppCompatActivity
import androidx.lifecycle.lifecycleScope
import fr.eosuivi.elevage.databinding.ActivityMainBinding
import kotlinx.coroutines.launch

/**
 * Écran principal : l'application EO-Suivi telle qu'elle est servie par le
 * serveur de l'élevage, dans une WebView.
 *
 * Le choix de la WebView n'est pas un raccourci : tous les écrans, toute la
 * logique métier et toutes les mises à jour restent côté serveur. Mettre à
 * jour le serveur met à jour l'application sur tous les téléphones, sans
 * store et sans réinstallation.
 */
class MainActivity : AppCompatActivity() {

    private lateinit var vue: ActivityMainBinding
    private lateinit var reglages: Reglages
    private var fichierChoisi: ValueCallback<Array<Uri>>? = null

    /** Sélecteur de fichier pour les imports CSV et PDF de l'application. */
    private val choixFichier = registerForActivityResult(ActivityResultContracts.StartActivityForResult()) { resultat ->
        val uris = WebChromeClient.FileChooserParams.parseResult(resultat.resultCode, resultat.data)
        fichierChoisi?.onReceiveValue(uris)
        fichierChoisi = null
    }

    private val configuration = registerForActivityResult(ActivityResultContracts.StartActivityForResult()) {
        ouvrirServeur()
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        vue = ActivityMainBinding.inflate(layoutInflater)
        setContentView(vue.root)
        reglages = Reglages(this)

        configurerWebView()

        vue.rafraichir.setOnRefreshListener { vue.web.reload() }
        vue.boutonReessayer.setOnClickListener { ouvrirServeur() }
        vue.boutonChangerServeur.setOnClickListener { ouvrirConnexion() }

        // Le bouton retour d'Android suit l'historique de navigation du site
        // avant de quitter l'application : sinon chaque retour ferme tout.
        onBackPressedDispatcher.addCallback(this, object : OnBackPressedCallback(true) {
            override fun handleOnBackPressed() {
                if (vue.web.canGoBack()) vue.web.goBack() else finish()
            }
        })

        if (reglages.vierge()) ouvrirConnexion() else ouvrirServeur()
    }

    @Suppress("SetJavaScriptEnabled")
    private fun configurerWebView() {
        with(vue.web.settings) {
            javaScriptEnabled = true
            domStorageEnabled = true
            // Le serveur envoie déjà une mise en page adaptée au mobile : ne
            // pas laisser la WebView appliquer son propre zoom « bureau ».
            useWideViewPort = true
            loadWithOverviewMode = true
            setSupportZoom(true)
            builtInZoomControls = true
            displayZoomControls = false
            cacheMode = WebSettings.LOAD_DEFAULT
        }
        CookieManager.getInstance().setAcceptCookie(true)
        CookieManager.getInstance().setAcceptThirdPartyCookies(vue.web, false)

        vue.web.webViewClient = object : WebViewClient() {
            override fun shouldOverrideUrlLoading(view: WebView, requete: WebResourceRequest): Boolean {
                val url = requete.url
                val serveur = Uri.parse(vue.web.url ?: return false)
                // Un lien vers l'extérieur (site d'un fournisseur, tel:, mailto:)
                // part dans le navigateur ou l'application dédiée, il n'a rien
                // à faire dans la coquille.
                return if (url.host != null && url.host == serveur.host) {
                    false
                } else {
                    runCatching { startActivity(Intent(Intent.ACTION_VIEW, url)) }.isSuccess
                }
            }

            override fun onPageFinished(view: WebView, url: String) {
                vue.rafraichir.isRefreshing = false
                vue.chargement.visibility = View.GONE
            }

            override fun onReceivedError(view: WebView, requete: WebResourceRequest, erreur: WebResourceError) {
                // Seule l'erreur de la page principale mérite l'écran d'échec :
                // une image manquante ne doit pas masquer une page utilisable.
                if (requete.isForMainFrame) afficherErreur(erreur.description?.toString())
            }
        }

        vue.web.webChromeClient = object : WebChromeClient() {
            override fun onShowFileChooser(
                view: WebView,
                rappel: ValueCallback<Array<Uri>>,
                parametres: FileChooserParams,
            ): Boolean {
                fichierChoisi?.onReceiveValue(null)
                fichierChoisi = rappel
                return runCatching { choixFichier.launch(parametres.createIntent()); true }
                    .getOrElse { fichierChoisi = null; false }
            }

            override fun onProgressChanged(view: WebView, avancement: Int) {
                vue.chargement.progress = avancement
            }
        }

        // Exports CSV et PDF : sans ce relais, un lien de téléchargement ne
        // fait rien du tout dans une WebView.
        vue.web.setDownloadListener(DownloadListener { url, agent, disposition, type, _ ->
            runCatching {
                val requete = DownloadManager.Request(Uri.parse(url)).apply {
                    setMimeType(type)
                    addRequestHeader("User-Agent", agent)
                    addRequestHeader("Cookie", CookieManager.getInstance().getCookie(url).orEmpty())
                    setNotificationVisibility(DownloadManager.Request.VISIBILITY_VISIBLE_NOTIFY_COMPLETED)
                    val nom = android.webkit.URLUtil.guessFileName(url, disposition, type)
                    setDestinationInExternalPublicDir(Environment.DIRECTORY_DOWNLOADS, nom)
                }
                (getSystemService(Context.DOWNLOAD_SERVICE) as DownloadManager).enqueue(requete)
            }
        })
    }

    private fun ouvrirServeur() {
        if (reglages.vierge()) {
            ouvrirConnexion()
            return
        }
        vue.erreur.visibility = View.GONE
        vue.web.visibility = View.VISIBLE
        vue.chargement.visibility = View.VISIBLE
        lifecycleScope.launch {
            val adresse = Joignable.choisir(reglages.adresseInterne, reglages.adresseExterne)
            if (adresse == null) ouvrirConnexion() else vue.web.loadUrl(adresse)
        }
    }

    private fun afficherErreur(message: String?) {
        vue.chargement.visibility = View.GONE
        vue.rafraichir.isRefreshing = false
        vue.web.visibility = View.GONE
        vue.erreur.visibility = View.VISIBLE
        vue.messageErreur.text = getString(
            R.string.serveur_injoignable,
            reglages.adresseInterne ?: reglages.adresseExterne.orEmpty(),
            message.orEmpty(),
        )
    }

    private fun ouvrirConnexion() {
        configuration.launch(Intent(this, ConnexionActivity::class.java))
    }

    override fun onCreateOptionsMenu(menu: Menu): Boolean {
        menuInflater.inflate(R.menu.principal, menu)
        return true
    }

    override fun onOptionsItemSelected(item: MenuItem): Boolean = when (item.itemId) {
        R.id.action_changer_serveur -> { ouvrirConnexion(); true }
        R.id.action_recharger -> { vue.web.reload(); true }
        else -> super.onOptionsItemSelected(item)
    }
}
