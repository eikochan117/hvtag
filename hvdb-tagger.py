#!/usr/bin/env python

from mutagen.easyid3 import *
import os
import sys
import re
import requests
from bs4 import BeautifulSoup

cwd = os.getcwd()
rjcode = os.path.basename(cwd)
splitChar = sys.argv[1]

bl_tags = ["non-mega link", "broken link", "mp3 only", "outdated"]


print("collecting HVDB data... ")
url = "https://hvdb.me/Dashboard/WorkDetails/" + rjcode
txt = requests.get(url)
soup = BeautifulSoup(txt.text, "html.parser")

album = soup.find("label", id="circleLabel").get_text().strip()
print("Album : " + album)
circle = soup.find("a", href=re.compile("CircleWorks")).get_text().split("/")[0].strip()
print("Circle : " + circle)
print("tags:")
tags_q = soup.find_all("a", href=re.compile("TagWorks"))
tags = list()
for t in tags_q:
    tt = t.get_text()
    if not tt in bl_tags:
        tags.append(tt)
print(tags)
print("CVs:")
cvs_q = soup.find_all("a", href=re.compile("CVWorks"))
cvs = list()
for cv in cvs_q:
    cvs.append(cv.get_text())
print(cvs)

# urlimg = "https://hvdb.me/WorkImages/RJ" + rjcode + ".jpg"
# img = requests.get(urlimg).content
# with open("folder.jpg", "wb") as f:
#     f.write(img)

files = [f for f in os.listdir(cwd) if f.endswith(".mp3")]

for f in files:
    fname = f.replace(".mp3", "")
    splits = fname.split(splitChar)
    num = re.sub(r'\D', '', splits[0])
    trackName = splits[1]
    print(f)
    print("   Tr." + num + " : " + trackName)
    m = EasyID3(f)
    m["tracknumber"] = num
    m["title"] = trackName
    m["album"] = album
    m["artist"] = "/".join(cvs)
    m["genre"] = "/".join(tags)
    m["albumartist"] = circle
    m["titlesort"] = rjcode
    m.save()
