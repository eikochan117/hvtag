#!/usr/bin/env python

from mutagen.easyid3 import *
import mutagen
import os
import re
import requests
import argparse
import shutil
import yaml

import asyncio
from dlsite_async import DlsiteAPI

asyncio.set_event_loop_policy(asyncio.WindowsSelectorEventLoopPolicy())

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

dictionary = []
circles = dict()

def lookForCircle(circle):
    if circle in circles:
        return circles[circle]
    else:
        print("collecting real name of circle "+ circle + "...")
        circleDl = asyncio.run(query_circle(circle))
        circles[circle] = circleDl.maker_name
        print("Done, it's " + circleDl.maker_name)
        return circleDl.maker_name

def convertTag(t):
    if t in dictionary : 
        return dictionary[t]
    return t

async def query_work(rj):
    async with DlsiteAPI(locale="en_US") as api:
        return await api.get_work(rj)

async def query_circle(rg):
    async with DlsiteAPI() as api:
        return await api.get_circle(rg)

def tag(cw, args):
    rjcode = os.path.basename(cw)
    print("collecting DLsite data...")
    try :
        work = asyncio.run(query_work(rjcode))
    except : 
        print("Error while looking for data on this work. It may have been unlisted.")
        return
    album = work.work_name
    
    circle = lookForCircle(work.maker_id)
    cvs = list()
    if work.voice_actor != None:
        for cv in work.voice_actor:
            cvs.append(cv)
    tags = list()
    if work.genre != None:
        for tag in work.genre:
            tags.append(convertTag(tag.lower()))
    files = [f for f in os.listdir(cw) if f.endswith(".mp3")]

    for f in files:
        fname = f.replace(".mp3", "")
        if not args["tags"]:
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
                # trackName = splits[trackNameIndex]
                trackName = fname.replace(splits[args["index"]] + splitChar, "")
                num = re.sub(r'\D', '', splits[args["index"]])
            print(f)
            print("   Tr." + num + " : " + trackName)
        filePath = cw + "/" + f
        try :
            m = EasyID3(filePath)
        except mutagen.id3.ID3NoHeaderError:
            m = mutagen.File(filePath, easy=True)
            m.add_tags()
        if not args["tags"]:
            m["tracknumber"] = num
            m["title"] = trackName
        m["album"] = album
        if len(cvs) == 0 :
            m["artist"] = "Missing CVs"
        else :
            m["artist"] = "\x00".join(cvs)
        if len(tags) == 0:
            m["genre"] = "Missing tags"
        else:
            m["genre"] = "\x00".join(tags)
        m["albumartist"] = circle
        m["albumsort"] = rjcode
        date = work.regist_date
        m["date"] = str(date.year) + "-" + str(date.month) + "-" + str(date.day)
        m.save()
    if args["image"]:
        print("Downloading work image...")
        url = work.work_image.replace("//", "https://")
        img = requests.get(url, stream=True)
        with open(cw + "/folder.jpeg", "wb") as f:
            shutil.copyfileobj(img.raw, f)
        print("Done.")
    open(cw + "/.tagged", 'a').close()
    if args["move"] != "":
        print("Moving " + cw + " to " + args["move"] + " ...")
        newPath = shutil.move(cw, args["move"])
        print("Done.")

if __name__ == "__main__":

    parser = argparse.ArgumentParser(
        description="Tag ASMR works using tags from Dlsite."
    )

    parser.add_argument("--batch", "-b", action="store_true", help="Process multiple works delimited by folder, named according to their RJ code.")
    parser.add_argument("--force", "-f", action="store_true", help="Force tagging even with .tagged file present.")
    parser.add_argument("--clean", "-c", action="store_true", help="Remove .tagged files.")
    parser.add_argument("--tags", "-t", action="store_true", help="Keep track number, only update tags.", default=False)
    parser.add_argument("--no-dict", action="store_true", help="Process tagging regardless of presence of dictionary.yaml file.", default=False)
    parser.add_argument("--image", action="store_true", help="Add/Replace folder.jpeg with Dlsite's work image.", default=False)
    parser.add_argument("--move", "-m", type=str, help="Move tagged folder to destination.", default="")

    singleParser = argparse.ArgumentParser()
    singleParser.add_argument("--remove", "-r", type=str, help="Filter out text from title to parse track number", default="")
    singleParser.add_argument("--first", "-1", action="store_true", help="Use left-most numeric character as track title.")
    singleParser.add_argument("--space", "-s", action="store_true", help="Use space as spliter")
    singleParser.add_argument("--wide", "-w", action="store_true", help="Convert wide number")
    singleParser.add_argument("--index", "-i", type=int, help="Specify split index", default=0)
    singleParser.add_argument("--move", "-m", type=str, help="Move tagged folder to destination.", default="")
    singleParser.add_argument("split", default="\x00")

    kwargs = vars(parser.parse_args())
    cwd = os.getcwd()

    if kwargs["batch"] :
        if os.path.isfile("./dictionary.yaml") :
            dictionary = yaml.safe_load(open("./dictionary.yaml", "r"))
        elif kwargs["no-dict"]:
            print("No dictionary.yaml found in current directory, skipping...")
        else:
            print("No dictionary.yaml found in current directory, use option '--no-dict' to process anyway. Exiting.")
            exit()
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
                        if not kwargs["tags"] :
                            print("Please input the separator character between track number and title.")
                            print("(Available commands : --remove, --first, --space, --wide, --index)")
                            args = vars(singleParser.parse_args(input().split(" ")))
                        else :
                            args = kwargs
                        args["tags"] = kwargs["tags"]
                        args["image"] = kwargs["image"]
                        if kwargs["move"] != "":
                            args["move"] = kwargs["move"]
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


