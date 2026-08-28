-- Une ligne par lot économique : ne jamais compter l'apport complet dans
-- chacune des bandes. Les imports récents sont déjà stockés lot par lot.
CREATE VIEW IF NOT EXISTS ventelot AS
WITH source AS (
 SELECT v.*,CASE WHEN json_valid(v.lots_json) AND json_type(v.lots_json)='array' THEN v.lots_json ELSE '[]' END AS lots_valides FROM venteapport v
), classified AS (
 SELECT s.*,CASE WHEN json_array_length(lots_valides)>1 AND nb_porcs=(SELECT SUM(CAST(json_extract(j.value,'$.nb_porcs') AS INTEGER)) FROM json_each(lots_valides) j) THEN 1 ELSE 0 END AS multi FROM source s
)
SELECT v.id,v.date,v.num_apport,CAST(j.key AS INTEGER) AS lot_index,
 json_extract(j.value,'$.ref') AS lot_ref,
 COALESCE(CAST(json_extract(j.value,'$.bande_id') AS INTEGER),v.bande_id) AS bande_id,
 CAST(json_extract(j.value,'$.nb_porcs') AS INTEGER) AS nb_porcs,
 CAST(json_extract(j.value,'$.poids') AS REAL) AS poids_total,
 CAST(json_extract(j.value,'$.montant_ht') AS REAL) AS montant_ht,
 CAST(json_extract(j.value,'$.muscle_lot') AS REAL) AS tmp,
 1 AS is_legacy
FROM classified v,json_each(v.lots_valides) j WHERE v.multi=1
UNION ALL
SELECT id,date,num_apport,-1,frappe,bande_id,nb_porcs,poids_total,montant_ht,tmp,0 FROM classified WHERE multi=0;
