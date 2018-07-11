#!/usr/bin/env python3

# This script could be used for actix-web multipart example test
# just start server and run client.py

import asyncio
import aiofiles
import aiohttp

file_name = 'test.png'
url = 'http://localhost:8080/upload'

async def file_sender(file_name=None):
    async with aiofiles.open(file_name, 'rb') as f:
        chunk = await f.read(64*1024)
        while chunk:
            yield chunk
            chunk = await f.read(64*1024)


async def req():
    async with aiohttp.ClientSession() as session:
        data = aiohttp.FormData(quote_fields=False)
        data.add_field("files[]", file_sender(file_name=file_name), filename="image1.png")
        data.add_field("files[]", file_sender(file_name=file_name), filename="image2.png")
        data.add_field("files[]", file_sender(file_name=file_name), filename="image3.png")
        data.add_field("Hey", "hi")
        data.add_field("Hi[One]", "1")
        data.add_field("Hi[Two]", "2.0")

        async with session.post(url, data=data) as resp:
            text = await resp.text()
            print(text)
            assert 201 == resp.status


loop = asyncio.get_event_loop()
loop.run_until_complete(req())
