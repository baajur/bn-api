const supertest = require('supertest');
const expect = require('chai').expect;
const mocha = require('mocha');
const tv4 = require('tv4');
const fs = require('fs');
const pm = require('../pm');
const debug = require("debug");
var log = debug('bn-api');
const users = require("../../helpers/user");

const baseUrl = supertest(pm.environment.get('server'));

const apiEndPoint = '/users/email_only';


var response;
var responseBody;


const post = async function (request_body) {
    return baseUrl
        .post(pm.substitute(apiEndPoint))
        .set('Accept', 'application/json')
        .set('Content-Type', 'application/json')

        .send(pm.substitute(request_body));
};
const requestBody = `{
	"first_name":"${users.makeid(8)}",
	"last_name":"${users.makeid(8)}",
	"email":"${users.makeid(8)}@localhost"
	
	}`;

describe('Guest - Register email only', function () {
    before(async function () {
        response = await post(requestBody);
        log(response.request.header);
        log(response.request.url);
        log(response.request._data);
        log(response.request.method);
        responseBody = JSON.stringify(response.body);
        //log(pm);
        log(response.status);
        console.log(responseBody);
    });

    after(async function () {
        // add after methods

    });

    it("should be 201", function () {
        expect(response.status).to.equal(201);
    })


});


