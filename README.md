# hvtag
hvtag but rewritten in rust. Tag audio files using their RJ code according to their DLsite page.

## Modules
*dl_site* : Everything needed to gather data from Dlsite, via their API or by scrapping.
*tagger* : Code to tag audio (mp3) files
*custom_tags* : Database (SQL/DuckDB for the fun of it) handling conversion of tags of different sources to custom ones
*converter* : Code to convert audio files to mp3/320kbps
*renamer* : Code to re-organize automatically files to follow a same pattern (tbd)

For the DB thing, allow to import .yaml file 
