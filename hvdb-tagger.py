#!/usr/bin/env python

from mutagen.easyid3 import *
import os
import sys
import re
import requests
from bs4 import BeautifulSoup

bl_tags = ["non-mega link", "broken link", "mp3 only", "outdated"]

def tag(cw, command):
    rjcode = os.path.basename(cw)
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
        if "N/A" in cv.get_text():
            cvs.append("Missing CV")
        else:
            cvs.append(cv.get_text())
    print(cvs)

    # urlimg = "https://hvdb.me/WorkImages/RJ" + rjcode + ".jpg"
    # img = requests.get(urlimg).content
    # with open("folder.jpg", "wb") as f:
    #     f.write(img)

    files = [f for f in os.listdir(cw) if f.endswith(".mp3")]

    for f in files:
        fname = f.replace(".mp3", "")
        cs = command.split(" ")
        useNoname = False
        useFirstChar = False
        splitChar = command
        i = 0
        while i < len(cs):
            c = cs[i]
            if "--remove" in c:
                i += 1
                ss = cs[i]
                fname = fname.replace(ss, "")
            elif "--no-title" in c:
                useNoname = True
            elif "--first-char" in c:
                useFirstChar = True
            elif "--space" in c:
                splitChar = " "
            elif i == len(cs) - 1:
                splitChar = c
            i += 1

        num = "1"
        trackName = fname
        if useNoname:
            if len(files) > 1:
                num = re.sub(r'\D', '', fname)
        elif useFirstChar:
            num = trackName[0]
        else:
            splits = fname.split(splitChar)
            trackName = splits[1]
            num = re.sub(r'\D', '', splits[0])

        print(f)
        print("   Tr." + num + " : " + trackName)
        m = EasyID3(cw + "/" + f)
        m["tracknumber"] = num
        m["title"] = trackName
        m["album"] = album
        m["artist"] = "/".join(cvs)
        m["genre"] = "/".join(tags)
        m["albumartist"] = circle
        m["titlesort"] = rjcode
        m.save()
    open(cw + "/.tagged", 'a').close()


mode = "single"
if  "--batch" in sys.argv:
    mode = "batch"

if  "--clean" in sys.argv:
    mode = "clean"

cwd = os.getcwd()
splitChar = sys.argv[1]

if mode == "batch":
    folders = [f for f in os.listdir(cwd) if os.path.isdir(os.path.join(cwd, f))]
    for folder in folders:
        if "RJ" in folder:
            print(folder)
            if os.path.isfile(cwd + "/" + folder + "/.tagged"):
                print("Folder " + folder + " already processed.")
            else:
                files = [f for f in os.listdir(cwd + "/" + folder) if f.endswith(".mp3")]
                if len(files) > 0 :
                    print(files[0])
                    command = input("Command ([--remove <text>, --no-title, --first-char, --wide] <separator>) : ")
                    tag(folder, command)
                else:
                    print("No valid file found in " + folder + " !")
elif mode == "clean":
    folders = [f for f in os.listdir(cwd) if os.path.isdir(os.path.join(cwd, f))]
    for folder in folders:
        if "RJ" in folder:
            print(folder)
            if os.path.isfile(cwd + "/" + folder + "/.tagged"):
                os.remove(cwd + "/" + folder + "/.tagged")
    print("done")
else:
    tag(cwd, splitChar)


