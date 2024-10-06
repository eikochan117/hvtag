#!/usr/bin/env python

from mutagen.easyid3 import *
import mutagen
import os
import sys
import re
import requests
import argparse
from bs4 import BeautifulSoup
def convertWide(t):
    t = t.replace("０", "0")
    t = t.replace("１", "1")
    t = t.replace("２", "2")
    t = t.replace("３", "3")
    t = t.replace("４", "4")
    t = t.replace("５", "5")
    t = t.replace("６", "6")
    t = t.replace("７", "7")
    t = t.replace("８", "8")
    t = t.replace("９", "9")
    t = t.replace("：", ":")
    return t

blacklistedTags = ["non-mega link", "broken link", "mp3 only", "outdated"]
jpregex = r"[\u3040-\u30ff\u3400-\u4dbf\u4e00-\u9fff\uf900-\ufaff\uff66-\uff9f]"


def tag(cw, args):
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
        if not tt in blacklistedTags:
            tags.append(tt)
    print(tags)
    print("CVs:")
    cvs_q = soup.find_all("a", href=re.compile("CVWorks"))
    cvs = list()
    for cv in cvs_q:
        name = cv.get_text()
        if "N/A" in name:
            cvs.append("Missing CV")
        else:
            if not args["jp"] or re.match(jpregex, name):
                cvs.append(name)
    print(cvs)

    files = [f for f in os.listdir(cw) if f.endswith(".mp3")]

    for f in files:
        fname = f.replace(".mp3", "")
        useNoname = args["first"]
        useFirstChar = False
        splitChar = args["split"]

        if len(args["remove"]) > 0 :
            fname = fname.replace(args["remove"], "")
        if args["space"]:
            splitChar = " "
        if args["wide"]:
            fname = convertWide(fname)

        num = "0"
        trackName = fname
        if useNoname:
            if len(files) > 1:
                num = re.sub(r'\D', '', fname)
        elif useFirstChar:
            i = 0
            while f[i].isdigit():
                i = i + 1
            num = f[0:i]
        else:
            splits = fname.split(splitChar)
            trackNameIndex = min(args["index"] + 1, len(splits) - 1)
            trackName = splits[trackNameIndex]
            num = re.sub(r'\D', '', splits[args["index"]])

        print(f)
        print("   Tr." + num + " : " + trackName)
        filePath = cw + "/" + f
        try : 
            m = EasyID3(filePath)
        except mutagen.id3.ID3NoHeaderError:
            m = mutagen.File(filePath, easy=True)
            m.add_tags()
        m["tracknumber"] = num
        m["title"] = trackName
        m["album"] = album
        m["artist"] = "\x00".join(cvs)
        m["genre"] = "\x00".join(tags)
        m["albumartist"] = circle
        m["titlesort"] = rjcode
        m.save()
    open(cw + "/.tagged", 'a').close()


if __name__ == "__main__":

    parser = argparse.ArgumentParser(
        description="Tag ASMR works using tags from HVDB."
    )

    parser.add_argument("--batch", "-b", action="store_true", help="Process multiple works delimited by folder, named according to their RJ code.")
    parser.add_argument("--force", "-f", action="store_true", help="Force tagging even with .tagged file present.")
    parser.add_argument("--clean", "-c", action="store_true", help="Remove .tagged files.")
    parser.add_argument("--jp", "-j", action="store_true", help="Only keep JP names.")

    singleParser = argparse.ArgumentParser()
    singleParser.add_argument("--remove", "-r", type=str, help="Filter out text from title to parse track number", default="")
    singleParser.add_argument("--first", "-1", action="store_true", help="Use left-most numeric character as track title.")
    singleParser.add_argument("--space", "-s", action="store_true", help="Use space as spliter")
    singleParser.add_argument("--wide", "-w", action="store_true", help="Convert wide number")
    singleParser.add_argument("--index", "-i", type=int, help="Specify split index", default=0)
    singleParser.add_argument("--jp", "-j", action="store_true", help="Only keep JP names.")
    singleParser.add_argument("split", default="\x00")

    kwargs = vars(parser.parse_args())
    cwd = os.getcwd()

    if kwargs["batch"] :
        folders = [f for f in os.listdir(cwd) if os.path.isdir(os.path.join(cwd, f))]
        for folder in folders:
            if "RJ" in folder:
                print(folder)
                if not kwargs["force"] and os.path.isfile(cwd + "/" + folder + "/.tagged"):
                    print("Folder " + folder + " already processed.")
                else:
                    files = [f for f in os.listdir(cwd + "/" + folder) if f.endswith(".mp3")]
                    if len(files) > 0 :
                        print(files[0])
                        args = vars(singleParser.parse_args(input().split(" ")))
                        if kwargs["jp"]:
                            args["jp"] = True
                        tag(folder, args)
                    else:
                        print("No valid file found in " + folder + " !")
    elif kwargs["clean"]:
        folders = [f for f in os.listdir(cwd) if os.path.isdir(os.path.join(cwd, f))]
        for folder in folders:
            if "RJ" in folder:
                print(folder)
                if os.path.isfile(cwd + "/" + folder + "/.tagged"):
                    os.remove(cwd + "/" + folder + "/.tagged")
        print("Done")
    else:
        rj = cwd.split("\\")[-1]
        kwargs = vars(singleParser.parse_args())
        tag(rj, kwargs)


