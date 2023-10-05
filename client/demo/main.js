// deno-fmt-ignore-file
// deno-lint-ignore-file
// This code was bundled using `deno bundle` and it's not recommended to edit it manually

var Challenge;
(function(Challenge) {
    var PREFIX = Challenge.PREFIX = [
        102,
        108,
        101,
        101,
        107
    ];
    function encode(frame) {
        const view = new Uint8Array(37);
        view.set(PREFIX);
        view.set(frame.challenge, 5);
        return view.buffer;
    }
    Challenge.encode = encode;
    function decode(payload) {
        if (payload.byteLength != 37) {
            return undefined;
        }
        const u8 = new Uint8Array(payload);
        for(let i = 0; i < PREFIX.length; ++i){
            if (u8[i] != PREFIX[i]) {
                return undefined;
            }
        }
        return {
            challenge: new Uint8Array(payload, 5, 32)
        };
    }
    Challenge.decode = decode;
})(Challenge || (Challenge = {}));
var HandshakeRequest;
(function(HandshakeRequest) {
    let Tag;
    (function(Tag) {
        Tag[Tag["Handshake"] = 0] = "Handshake";
        Tag[Tag["JoinRequest"] = 1] = "JoinRequest";
    })(Tag = HandshakeRequest.Tag || (HandshakeRequest.Tag = {}));
    function encode(frame) {
        if (frame.tag === Tag.Handshake) {
            let writer;
            if (frame.retry === undefined) {
                writer = new Writer(149);
                writer.putU8(0x00);
            } else {
                writer = new Writer(157);
                writer.putU8(0x01);
                writer.putU64(frame.retry);
            }
            writer.putU32(frame.service);
            writer.put(frame.pk);
            writer.put(frame.pop);
            return writer.getBuffer();
        }
        if (frame.tag === Tag.JoinRequest) {
            const u8 = new Uint8Array(49);
            u8[0] = 0x02;
            u8.set(frame.accessToken, 0);
            return u8.buffer;
        }
        throw new Error("Unsupported");
    }
    HandshakeRequest.encode = encode;
    function decode(payload) {
        const reader = new Reader(payload);
        const tag = reader.getU8();
        if (tag === 0x00) {
            if (payload.byteLength !== 149) {
                return;
            }
            return {
                tag: Tag.Handshake,
                service: reader.getU32(),
                pk: reader.get(96),
                pop: reader.get(48)
            };
        }
        if (tag === 0x01) {
            if (payload.byteLength !== 157) {
                return;
            }
            return {
                tag: Tag.Handshake,
                retry: reader.getU64(),
                service: reader.getU32(),
                pk: reader.get(96),
                pop: reader.get(48)
            };
        }
        if (tag === 0x02) {
            if (payload.byteLength !== 49) {
                return;
            }
            return {
                tag: Tag.JoinRequest,
                accessToken: reader.get(49)
            };
        }
    }
    HandshakeRequest.decode = decode;
})(HandshakeRequest || (HandshakeRequest = {}));
var HandshakeResponse;
(function(HandshakeResponse) {
    function encode(frame) {
        const writer = new Writer(96);
        writer.put(frame.pk);
        writer.put(frame.pop);
        return writer.getBuffer();
    }
    HandshakeResponse.encode = encode;
    function decode(payload) {
        if (payload.byteLength != 96) {
            return;
        }
        const reader = new Reader(payload);
        const pk = reader.get(32);
        const pop = reader.get(64);
        return {
            pk,
            pop
        };
    }
    HandshakeResponse.decode = decode;
})(HandshakeResponse || (HandshakeResponse = {}));
var Request;
(function(Request) {
    let Tag;
    (function(Tag) {
        Tag[Tag["ServicePayload"] = 0] = "ServicePayload";
        Tag[Tag["AccessToken"] = 1] = "AccessToken";
        Tag[Tag["ExtendAccessToken"] = 2] = "ExtendAccessToken";
    })(Tag = Request.Tag || (Request.Tag = {}));
    function encode(frame) {
        if (frame.tag == Tag.ServicePayload) {
            const u8 = new Uint8Array(frame.bytes.byteLength + 1);
            u8[0] = 0x00;
            u8.set(frame.bytes, 1);
            return u8.buffer;
        }
        if (frame.tag == Tag.AccessToken) {
            const writer = new Writer(9);
            writer.putU8(0x01);
            writer.putU64(frame.ttl);
            return writer.getBuffer();
        }
        if (frame.tag == Tag.ExtendAccessToken) {
            const writer = new Writer(9);
            writer.putU8(0x02);
            writer.putU64(frame.ttl);
            return writer.getBuffer();
        }
        throw new Error("Not supported.");
    }
    Request.encode = encode;
    function decode(payload) {
        const reader = new Reader(payload);
        const tag = reader.getU8();
        if (tag == 0) {
            return {
                tag: Tag.ServicePayload,
                bytes: new Uint8Array(payload).slice(1)
            };
        }
        if (tag == 1) {
            if (payload.byteLength != 9) {
                return;
            }
            return {
                tag: Tag.AccessToken,
                ttl: reader.getU64()
            };
        }
        if (tag == 2) {
            if (payload.byteLength != 9) {
                return;
            }
            return {
                tag: Tag.ExtendAccessToken,
                ttl: reader.getU64()
            };
        }
    }
    Request.decode = decode;
})(Request || (Request = {}));
var Response;
(function(Response) {
    let Tag;
    (function(Tag) {
        Tag[Tag["ServicePayload"] = 0] = "ServicePayload";
        Tag[Tag["ChunkedServicePayload"] = 1] = "ChunkedServicePayload";
        Tag[Tag["AccessToken"] = 2] = "AccessToken";
        Tag[Tag["Termination"] = 3] = "Termination";
    })(Tag = Response.Tag || (Response.Tag = {}));
    let TerminationReason;
    (function(TerminationReason) {
        TerminationReason[TerminationReason["Timeout"] = 0x80] = "Timeout";
        TerminationReason[TerminationReason["InvalidHandshake"] = 129] = "InvalidHandshake";
        TerminationReason[TerminationReason["InvalidToken"] = 130] = "InvalidToken";
        TerminationReason[TerminationReason["InvalidDeliveryAcknowledgment"] = 131] = "InvalidDeliveryAcknowledgment";
        TerminationReason[TerminationReason["InvalidService"] = 132] = "InvalidService";
        TerminationReason[TerminationReason["ServiceTerminated"] = 133] = "ServiceTerminated";
        TerminationReason[TerminationReason["ConnectionInUse"] = 134] = "ConnectionInUse";
        TerminationReason[TerminationReason["WrongPermssion"] = 135] = "WrongPermssion";
        TerminationReason[TerminationReason["Unknown"] = 0xFF] = "Unknown";
    })(TerminationReason = Response.TerminationReason || (Response.TerminationReason = {}));
    function encode(frame) {
        if (frame.tag === Tag.ServicePayload) {
            const u8 = new Uint8Array(frame.bytes.byteLength + 1);
            u8[0] = 0x00;
            u8.set(frame.bytes, 1);
            return u8.buffer;
        }
        if (frame.tag === Tag.ChunkedServicePayload) {
            const u8 = new Uint8Array(frame.bytes.byteLength + 1);
            u8[0] = 0x40;
            u8.set(frame.bytes, 1);
            return u8.buffer;
        }
        if (frame.tag === Tag.AccessToken) {
            const writer = new Writer(57);
            writer.putU8(0x01);
            writer.putU64(frame.ttl);
            writer.put(frame.accessToken);
            return writer.getBuffer();
        }
        if (frame.tag === Tag.Termination) {
            const u8 = new Uint8Array(1);
            u8[0] = frame.reason;
            return u8.buffer;
        }
        throw new Error("Unsupported");
    }
    Response.encode = encode;
    function decode(payload) {
        const reader = new Reader(payload);
        const tag = reader.getU8();
        if (tag == 0x00) {
            return {
                tag: Tag.ServicePayload,
                bytes: new Uint8Array(payload).slice(1)
            };
        }
        if (tag == 0x40) {
            return {
                tag: Tag.ChunkedServicePayload,
                bytes: new Uint8Array(payload).slice(1)
            };
        }
        if (tag == 0x01) {
            const ttl = reader.getU64();
            const accessToken = reader.get(64);
            return {
                tag: Tag.AccessToken,
                ttl,
                accessToken
            };
        }
        if (tag < 0x80) {
            throw new Error("Unsupported");
        }
        return {
            tag: Tag.Termination,
            reason: ({
                0: TerminationReason.Timeout,
                1: TerminationReason.InvalidHandshake,
                2: TerminationReason.InvalidToken,
                3: TerminationReason.InvalidDeliveryAcknowledgment,
                4: TerminationReason.InvalidService,
                5: TerminationReason.ServiceTerminated,
                6: TerminationReason.ConnectionInUse,
                7: TerminationReason.WrongPermssion
            })[tag - 0x80] || TerminationReason.Unknown
        };
    }
    Response.decode = decode;
})(Response || (Response = {}));
class Writer {
    buffer;
    view;
    cursor = 0;
    constructor(size){
        this.buffer = new Uint8Array(size);
        this.view = new DataView(this.buffer.buffer);
    }
    putU8(n) {
        this.view.setUint8(this.cursor, n);
        this.cursor += 1;
    }
    putU32(n) {
        this.view.setUint32(this.cursor, n);
        this.cursor += 4;
    }
    putU64(n) {
        const lo = n & 0xffffffff;
        const hi = n >> 4;
        this.view.setUint32(this.cursor, hi);
        this.view.setUint32(this.cursor + 4, lo);
        this.cursor += 8;
    }
    put(array) {
        this.buffer.set(array, this.cursor);
        this.cursor += array.length;
    }
    getBuffer() {
        if (this.cursor != this.buffer.byteLength) {
            console.warn("dead allocation");
            return this.buffer.buffer.slice(0, this.cursor);
        }
        return this.buffer.buffer;
    }
}
class Reader {
    buffer;
    view;
    cursor = 0;
    constructor(buffer){
        this.buffer = new Uint8Array(buffer);
        this.view = new DataView(buffer);
    }
    getU8() {
        return this.buffer[this.cursor++];
    }
    getU32() {
        const offset = this.cursor;
        this.cursor += 4;
        return this.view.getUint32(offset);
    }
    getU64() {
        const offset = this.cursor;
        this.cursor += 8;
        const hi = this.view.getUint32(offset);
        const lo = this.view.getUint32(offset + 4);
        return (hi << 4) + lo;
    }
    get(length) {
        const offset = this.cursor;
        this.cursor += length;
        return this.buffer.slice(offset, this.cursor);
    }
}
const video = document.querySelector("video");
document.getElementById("start").onclick = async ()=>{
    await startSession();
};
const queue = [];
let didReceiveFirstMessage = false;
let sourceBuffer;
let segCounter = 0;
let bytesRead = 0;
let getNextSent;
const pc = new RTCPeerConnection({
    iceServers: [
        {
            urls: "stun:stun.l.google.com:19302"
        }
    ]
});
const dataChan = pc.createDataChannel("fleek", {
    ordered: true
});
dataChan.onclose = ()=>{
    console.log("dataChan closed.");
};
dataChan.onopen = ()=>{
    console.log("dataChan opened.");
    dataChan.send(HandshakeRequest.encode({
        tag: HandshakeRequest.Tag.Handshake,
        service: 1,
        pk: new Uint8Array(96),
        pop: new Uint8Array(48)
    }));
};
dataChan.onmessage = (e)=>{
    if (!didReceiveFirstMessage) {
        didReceiveFirstMessage = true;
        if (sourceBuffer != undefined) {
            getNext();
        }
        return;
    }
    const decoded = Response.decode(e.data);
    if (decoded && (decoded.tag === Response.Tag.ServicePayload || decoded.tag === Response.Tag.ChunkedServicePayload)) {
        if (bytesRead == 0) {
            console.log(performance.measure("first-byte", {
                start: getNextSent,
                end: performance.now()
            }));
        }
        bytesRead += decoded.bytes.byteLength;
        appendBuffer(decoded.bytes);
        if (bytesRead >= 256 * 1024) {
            getNext();
            bytesRead = 0;
        }
    }
};
function appendBuffer(buffer) {
    if (sourceBuffer.updating || queue.length != 0) {
        queue.push(buffer);
        return;
    }
    if (getNextSent != undefined) {
        console.log(performance.measure("first-frame", {
            start: getNextSent,
            end: performance.now()
        }));
        getNextSent = undefined;
    }
    sourceBuffer.appendBuffer(buffer);
}
function getNext() {
    getNextSent = performance.now();
    const buffer = new ArrayBuffer(4);
    const view = new DataView(buffer);
    view.setUint32(0, segCounter++);
    dataChan.send(Request.encode({
        tag: Request.Tag.ServicePayload,
        bytes: new Uint8Array(buffer)
    }));
}
pc.oniceconnectionstatechange = ()=>{
    console.log("pc status: ", pc.iceConnectionState);
};
pc.onicecandidate = (e)=>{
    if (e.candidate === null) {
        console.log("pc description: ", pc.localDescription);
    }
};
pc.onnegotiationneeded = async (e)=>{
    try {
        console.log("connection negotiation ended", e);
        const d = await pc.createOffer();
        pc.setLocalDescription(d);
    } catch (e) {
        console.error("error: ", e);
    }
};
const startSession = async ()=>{
    console.log("sending sdp signal");
    const res = await fetch("http://localhost:4210/sdp", {
        method: "POST",
        headers: {
            "Content-Type": "application/json"
        },
        body: JSON.stringify(pc.localDescription)
    });
    const sd = await res.json();
    if (sd === "") {
        return alert("Session Description must not be empty");
    }
    console.log("got sd: ", sd);
    try {
        pc.setRemoteDescription(new RTCSessionDescription(sd));
    } catch (e) {
        alert(e);
    }
};
const mimeCodec = 'video/mp4; codecs="avc1.42E01E, mp4a.40.2"';
if ("MediaSource" in window && MediaSource.isTypeSupported(mimeCodec)) {
    const mediaSource = new MediaSource();
    video.src = URL.createObjectURL(mediaSource);
    mediaSource.addEventListener("sourceopen", sourceOpen);
} else {
    console.error("Unsupported MIME type or codec: ", mimeCodec);
}
function sourceOpen() {
    console.log(this.readyState);
    sourceBuffer = this.addSourceBuffer(mimeCodec);
    sourceBuffer.addEventListener("updateend", function(_) {
        if (queue.length != 0) {
            sourceBuffer.appendBuffer(queue.shift());
        }
    });
}
