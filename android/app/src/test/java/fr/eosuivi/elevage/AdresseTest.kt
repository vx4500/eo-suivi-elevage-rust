package fr.eosuivi.elevage

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * La normalisation d'adresse est le point où l'éleveur se trompe : c'est la
 * seule logique de l'application qui se teste sans téléphone, donc elle est
 * testée sérieusement.
 */
class AdresseTest {

    @Test
    fun `une IP privee devient du http avec le port du serveur`() {
        assertEquals("http://192.168.1.108:8080", Adresse.normaliser("192.168.1.108"))
        assertEquals("http://10.0.0.5:8080", Adresse.normaliser("10.0.0.5"))
        assertEquals("http://172.20.3.4:8080", Adresse.normaliser("172.20.3.4"))
    }

    @Test
    fun `un port deja saisi est respecte`() {
        assertEquals("http://192.168.1.108:9000", Adresse.normaliser("192.168.1.108:9000"))
    }

    @Test
    fun `un nom public passe en https sans port ajoute`() {
        assertEquals(
            "https://elevage.basse-chevrie.ovh",
            Adresse.normaliser("elevage.basse-chevrie.ovh"),
        )
    }

    @Test
    fun `un schema explicite prime sur la deduction`() {
        assertEquals("http://elevage.mon-domaine.fr", Adresse.normaliser("http://elevage.mon-domaine.fr"))
        assertEquals("https://192.168.1.108", Adresse.normaliser("https://192.168.1.108"))
    }

    @Test
    fun `les espaces le chemin et la barre finale sont ignores`() {
        assertEquals("http://192.168.1.108:8080", Adresse.normaliser("  192.168.1.108/  "))
        assertEquals("https://elevage.fr", Adresse.normaliser("https://elevage.fr/dashboard"))
    }

    @Test
    fun `une saisie vide ou absurde est refusee`() {
        assertNull(Adresse.normaliser(""))
        assertNull(Adresse.normaliser("   "))
        assertNull(Adresse.normaliser("ftp://192.168.1.108"))
        assertNull(Adresse.normaliser("mon adresse"))
    }

    @Test
    fun `les plages privees sont reconnues et les publiques non`() {
        assertTrue(Adresse.estLocale("192.168.0.1"))
        assertTrue(Adresse.estLocale("10.255.255.254"))
        assertTrue(Adresse.estLocale("172.16.0.1"))
        assertTrue(Adresse.estLocale("172.31.255.255"))
        assertTrue(Adresse.estLocale("169.254.1.1"))
        assertTrue(Adresse.estLocale("serveur.local"))
        assertTrue(Adresse.estLocale("localhost"))
        // 172.15 et 172.32 sont hors de la plage privée : une erreur classique.
        assertFalse(Adresse.estLocale("172.15.0.1"))
        assertFalse(Adresse.estLocale("172.32.0.1"))
        assertFalse(Adresse.estLocale("8.8.8.8"))
        assertFalse(Adresse.estLocale("elevage.basse-chevrie.ovh"))
        // Un octet hors bornes n'est pas une IP.
        assertFalse(Adresse.estLocale("192.168.1.999"))
    }

    @Test
    fun `un service decouvert donne une url http directe`() {
        assertEquals("http://192.168.1.108:8080", Adresse.depuisDecouverte("192.168.1.108", 8080))
    }
}
