-- Même définition que le schéma initial, vérifiée par un test. Aucune donnée supprimée.
DROP VIEW IF EXISTS portee_effectif;
CREATE VIEW IF NOT EXISTS portee_effectif AS
WITH mb AS (
 SELECT e.*,LEAD(date) OVER(PARTITION BY truie_id ORDER BY date,id) AS prochaine_mb
 FROM evenement e WHERE type='mise_bas'
), cycles AS (
 SELECT e.*,(SELECT s.id FROM evenement s WHERE s.truie_id=e.truie_id AND s.type='sevrage'
 AND (s.bande_id IS e.bande_id OR s.bande_id IS NULL OR e.bande_id IS NULL)
 AND s.date>=e.date AND (e.prochaine_mb IS NULL OR s.date<e.prochaine_mb)
 AND s.date<=date('now') ORDER BY s.date DESC,s.id DESC LIMIT 1) AS sevrage_id
 FROM mb e
), chiffres AS (
 SELECT e.id,e.truie_id,e.bande_id,e.date,e.prochaine_mb,e.sevrage_id,
 s.date AS date_sevrage,s.nb_sevres,s.poids_moyen,s.eld_entree,s.eld_sortie,
 COALESCE(e.nes_vifs,0) AS nes_vifs,
 COALESCE((SELECT SUM(a.nombre) FROM adoptionporcelet a WHERE a.destination_id=e.id AND a.date<=date('now')),e.adoptes,s.adoptes,0) AS adoptes,
 COALESCE((SELECT SUM(a.nombre) FROM adoptionporcelet a WHERE a.source_id=e.id AND a.date<=date('now')),e.retires,s.retires,0) AS retires,
 COALESCE((SELECT SUM(p.nb) FROM perteporcelet p WHERE p.truie_id=e.truie_id
 AND (p.evenement_id=e.id OR (p.evenement_id IS NULL
 AND (p.bande_id IS e.bande_id OR p.bande_id IS NULL OR e.bande_id IS NULL)
 AND p.date>=e.date AND (e.prochaine_mb IS NULL OR p.date<e.prochaine_mb)))
 AND p.date<=COALESCE(s.date,date('now'))),0) AS pertes,
 CASE WHEN e.sevrage_id IS NOT NULL OR e.prochaine_mb<=date('now') THEN 1 ELSE 0 END AS cloturee
 FROM cycles e LEFT JOIN evenement s ON s.id=e.sevrage_id
)
SELECT id,truie_id,bande_id,date,
 CASE WHEN cloturee=1 OR date>date('now') THEN 0 ELSE nes_vifs+adoptes-retires-pertes END AS presents,
 prochaine_mb,sevrage_id,date_sevrage,nb_sevres,poids_moyen,eld_entree,eld_sortie,adoptes,retires,pertes,cloturee,
 nes_vifs+adoptes-retires-pertes AS avant_sevrage
FROM chiffres;
