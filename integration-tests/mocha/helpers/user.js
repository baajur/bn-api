const {post,get} = require("./requests");
const pm = require('../test/pm');const debug=require('debug'); let log = debug('bn-api');

const expect = require('chai').expect;


const registerAndLogin = async function (saveVarName,dontSaveVars) {
    let email  ="user" + (new Date()).getTime() + Math.floor((Math.random() * 1000000)) + "@tari.com";
    if (!dontSaveVars) {
        pm.environment.set("last_email", email);
    }
    log(email);
    const response = await post('/users', `{
	"first_name":"${makeid(8)}",
	"last_name":"${makeid(8)}",
	"email":"${email}",
	"phone":"555",
	"password": "itsasecret"
}`);
    expect(response.status).to.equal(201);
    const responseBody = JSON.stringify(response.body);
    const json = JSON.parse(responseBody);

    if (!dontSaveVars) {
        pm.environment.set(saveVarName || "user_token", json.access_token);
    }
    return json.access_token;
};

const me  = async function(token) {
    const response = await get('/users/me', token);
    return JSON.parse(JSON.stringify(response.body));
};

module.exports = {
    registerAndLogin, me, makeid
};

function makeid(length) {
    var result           = '';
    var characters       = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz';
    var charactersLength = characters.length;
    for ( var i = 0; i < length; i++ ) {
        result += characters.charAt(Math.floor(Math.random() * charactersLength));
    }
    return result;
}
