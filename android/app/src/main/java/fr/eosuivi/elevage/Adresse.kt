package fr.eosuivi.elevage

/**
 * Normalisation des adresses de serveur saisies ou découvertes.
 *
 * Logique volontairement pure et sans dépendance Android : c'est la partie
 * qui se trompe le plus facilement (un éleveur tape « 192.168.1.108 » ou
 * « elevage.basse-chevrie.ovh/ » ou colle une URL complète), et c'est la
 * seule qu'on peut tester sans téléphone.
 */
object Adresse {

    /**
     * Transforme une saisie libre en URL utilisable, ou `null` si rien
     * d'exploitable.
     *
     * Règles :
     * - un schéma déjà présent est respecté ;
     * - sans schéma, on choisit `http` pour une adresse locale (IP privée ou
     *   nom en `.local`) et `https` pour un nom public — un élevage sans
     *   internet n'a pas de certificat, un élevage avec un domaine en a un ;
     * - le port par défaut 8080 est ajouté pour une adresse locale sans port,
     *   parce que c'est le port du serveur ; jamais pour un nom public, qui
     *   passe par un proxy en 443.
     */
    fun normaliser(saisie: String): String? {
        val brut = saisie.trim().trim('/')
        if (brut.isEmpty()) return null

        val avecSchema = brut.contains("://")
        val corps = if (avecSchema) brut.substringAfter("://") else brut
        val hote = corps.substringBefore('/').substringBefore('?')
        if (hote.isEmpty()) return null

        val hoteSansPort = hote.substringBeforeLast(':', hote).let {
            // « 192.168.1.108:8080 » → hôte « 192.168.1.108 » ; mais un hôte
            // sans port ne doit pas perdre un morceau de nom.
            if (hote.count { c -> c == ':' } == 1 && hote.substringAfterLast(':').all(Char::isDigit)) it else hote
        }
        if (!hoteValide(hoteSansPort)) return null

        val local = estLocale(hoteSansPort)
        val schema = if (avecSchema) brut.substringBefore("://") else if (local) "http" else "https"
        if (schema != "http" && schema != "https") return null

        val portPresent = hote != hoteSansPort
        val hoteFinal = if (!portPresent && local && schema == "http") "$hote:8080" else hote
        return "$schema://$hoteFinal"
    }

    /**
     * Vrai pour une adresse qui ne peut être atteinte que depuis le réseau
     * local : IPv4 privée (RFC 1918), lien-local, boucle locale, ou nom en
     * `.local` (mDNS). Sert à choisir `http` plutôt que `https`, et à décider
     * quelle adresse essayer en premier au démarrage.
     */
    fun estLocale(hote: String): Boolean {
        val nom = hote.lowercase()
        if (nom == "localhost" || nom.endsWith(".local")) return true
        val octets = nom.split('.')
        if (octets.size != 4 || octets.any { it.isEmpty() || !it.all(Char::isDigit) }) return false
        val valeurs = octets.map { it.toIntOrNull() ?: return false }
        if (valeurs.any { it !in 0..255 }) return false
        val (a, b) = valeurs[0] to valeurs[1]
        return a == 10 ||
            a == 127 ||
            (a == 192 && b == 168) ||
            (a == 172 && b in 16..31) ||
            (a == 169 && b == 254)
    }

    /** Hôte plausible : pas d'espace, pas de caractère interdit, non vide. */
    private fun hoteValide(hote: String): Boolean {
        if (hote.isEmpty() || hote.length > 253) return false
        return hote.all { it.isLetterOrDigit() || it == '.' || it == '-' || it == '_' }
    }

    /**
     * Construit l'URL d'un service découvert en mDNS. Le serveur s'annonce
     * toujours en clair sur le réseau local, d'où `http`.
     */
    fun depuisDecouverte(hote: String, port: Int): String = "http://$hote:$port"
}
