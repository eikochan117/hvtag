# hvtag
hvtag but rewritten in rust. Tag audio files using their RJ code according to their DLsite page.

## Modules
- *dlsite* : Everything needed to gather data from Dlsite, via their API or by scrapping.
- *tagger* : Code to tag audio (mp3) files
- *converter* : Code to convert audio files to mp3/320kbps
- *renamer* : Code to re-organize automatically files to follow a same pattern (tbd)
- *folders* : Code to keep track of scanned folders and their content
- *database* : All database related stuff

## Checklist of stuff to do
- ~~Scan existing library and keep track of their path, last update with Dlsite (never at first), rj code~~
- ~~Collect Dlsite tags for this existing library, ordering them in the database~~
- Allow renaming tags from Dlsite to custom ones
- Apply those tags to existing files
~~- Save query results of dlsite in the db to avoid unnecessary api spam and crawl~~
~~- Save CVs and Circles the same way~~
- Enable conversion of files to mp3
- Enable renaming like previous version of hvtag
- Try to implement smart name pattern parsing ?
